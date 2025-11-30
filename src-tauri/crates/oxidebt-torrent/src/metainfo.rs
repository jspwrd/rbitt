use crate::error::TorrentError;
use crate::info_hash::{InfoHash, InfoHashV1, InfoHashV2};
use bytes::Bytes;
use oxidebt_bencode::Value;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TorrentVersion {
    V1,
    V2,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub path: PathBuf,
    pub length: u64,
    pub pieces_root: Option<[u8; 32]>,
}

#[derive(Debug, Clone)]
pub enum FileTree {
    File {
        length: u64,
        pieces_root: Option<[u8; 32]>,
    },
    Directory(BTreeMap<String, FileTree>),
}

impl FileTree {
    fn from_bencode(value: &Value) -> Result<Self, TorrentError> {
        let dict = value.as_dict().ok_or(TorrentError::InvalidFieldType {
            field: "file_tree",
            expected: "dict",
        })?;

        if dict.contains_key(b"".as_slice()) {
            let file_info = dict.get(b"".as_slice()).unwrap().as_dict().ok_or(
                TorrentError::InvalidFieldType {
                    field: "file_info",
                    expected: "dict",
                },
            )?;

            let length = file_info
                .get(b"length".as_slice())
                .and_then(|v| v.as_integer())
                .ok_or(TorrentError::MissingField("length"))? as u64;

            let pieces_root = file_info.get(b"pieces root".as_slice()).and_then(|v| {
                v.as_bytes().and_then(|b| {
                    if b.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(b);
                        Some(arr)
                    } else {
                        None
                    }
                })
            });

            Ok(FileTree::File {
                length,
                pieces_root,
            })
        } else {
            let mut entries = BTreeMap::new();
            for (key, value) in dict {
                let name = std::str::from_utf8(key)
                    .map_err(|_| TorrentError::InvalidFilePath("invalid utf-8".into()))?
                    .to_string();
                entries.insert(name, FileTree::from_bencode(value)?);
            }
            Ok(FileTree::Directory(entries))
        }
    }

    pub fn flatten(&self, base: &std::path::Path) -> Vec<File> {
        let mut files = Vec::new();
        self.flatten_into(base, &mut files);
        files
    }

    fn flatten_into(&self, current_path: &std::path::Path, files: &mut Vec<File>) {
        match self {
            FileTree::File {
                length,
                pieces_root,
            } => {
                files.push(File {
                    path: current_path.to_path_buf(),
                    length: *length,
                    pieces_root: *pieces_root,
                });
            }
            FileTree::Directory(entries) => {
                for (name, tree) in entries {
                    let path = current_path.join(name);
                    tree.flatten_into(&path, files);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Info {
    pub name: String,
    pub piece_length: u64,
    pub pieces: Option<Bytes>,
    pub files: Vec<File>,
    pub total_length: u64,
    pub file_tree: Option<FileTree>,
    pub meta_version: Option<u32>,
    pub private: bool,
}

#[derive(Debug, Clone)]
pub struct Metainfo {
    pub info: Info,
    pub info_hash: InfoHash,
    pub announce: Option<String>,
    pub announce_list: Option<Vec<Vec<String>>>,
    pub creation_date: Option<i64>,
    pub comment: Option<String>,
    pub created_by: Option<String>,
    pub version: TorrentVersion,
    raw_info: Bytes,
}

impl Metainfo {
    pub fn from_bytes(data: &[u8]) -> Result<Self, TorrentError> {
        let (value, info_raw) = oxidebt_bencode::decode::decode_with_info_raw(data)?;

        let dict = value.as_dict().ok_or(TorrentError::InvalidFieldType {
            field: "root",
            expected: "dict",
        })?;

        let info_value = dict
            .get(b"info".as_slice())
            .ok_or(TorrentError::MissingField("info"))?;

        let info_raw = info_raw.ok_or(TorrentError::MissingField("info"))?;

        let info_dict = info_value.as_dict().ok_or(TorrentError::InvalidFieldType {
            field: "info",
            expected: "dict",
        })?;

        let name = info_dict
            .get(b"name".as_slice())
            .and_then(|v| v.as_str())
            .ok_or(TorrentError::MissingField("name"))?
            .to_string();

        let piece_length = info_dict
            .get(b"piece length".as_slice())
            .and_then(|v| v.as_integer())
            .ok_or(TorrentError::MissingField("piece length"))?;

        if piece_length <= 0 {
            return Err(TorrentError::InvalidPieceLength(piece_length));
        }
        let piece_length = piece_length as u64;

        let pieces = info_dict
            .get(b"pieces".as_slice())
            .and_then(|v| v.as_bytes())
            .cloned();

        if let Some(ref p) = pieces {
            if p.len() % 20 != 0 {
                return Err(TorrentError::InvalidPiecesLength(p.len()));
            }
        }

        let file_tree = info_dict
            .get(b"file tree".as_slice())
            .map(FileTree::from_bencode)
            .transpose()?;

        let meta_version = info_dict
            .get(b"meta version".as_slice())
            .and_then(|v| v.as_integer())
            .map(|v| v as u32);

        let private = info_dict
            .get(b"private".as_slice())
            .and_then(|v| v.as_integer())
            .map(|v| v == 1)
            .unwrap_or(false);

        let (files, total_length) = if let Some(ref ft) = file_tree {
            let files = ft.flatten(std::path::Path::new(&name));
            let total = files.iter().map(|f| f.length).sum();
            (files, total)
        } else if let Some(files_list) = info_dict.get(b"files".as_slice()) {
            parse_v1_files(&name, files_list)?
        } else {
            let length = info_dict
                .get(b"length".as_slice())
                .and_then(|v| v.as_integer())
                .ok_or(TorrentError::MissingField("length"))? as u64;

            let files = vec![File {
                path: PathBuf::from(&name),
                length,
                pieces_root: None,
            }];
            (files, length)
        };

        let version = match (pieces.is_some(), file_tree.is_some()) {
            (true, true) => TorrentVersion::Hybrid,
            (true, false) => TorrentVersion::V1,
            (false, true) => TorrentVersion::V2,
            (false, false) => return Err(TorrentError::MissingField("pieces or file tree")),
        };

        let info_hash = match version {
            TorrentVersion::V1 => InfoHash::V1(InfoHashV1::from_info_bytes(&info_raw)),
            TorrentVersion::V2 => InfoHash::V2(InfoHashV2::from_info_bytes(&info_raw)),
            TorrentVersion::Hybrid => InfoHash::Hybrid {
                v1: InfoHashV1::from_info_bytes(&info_raw),
                v2: InfoHashV2::from_info_bytes(&info_raw),
            },
        };

        let announce = dict
            .get(b"announce".as_slice())
            .and_then(|v| v.as_str())
            .map(String::from);

        let announce_list = dict
            .get(b"announce-list".as_slice())
            .and_then(|v| v.as_list())
            .map(|list| {
                list.iter()
                    .filter_map(|tier| {
                        tier.as_list().map(|urls| {
                            urls.iter()
                                .filter_map(|u| u.as_str().map(String::from))
                                .collect()
                        })
                    })
                    .collect()
            });

        let creation_date = dict
            .get(b"creation date".as_slice())
            .and_then(|v| v.as_integer());

        let comment = dict
            .get(b"comment".as_slice())
            .and_then(|v| v.as_str())
            .map(String::from);

        let created_by = dict
            .get(b"created by".as_slice())
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(Metainfo {
            info: Info {
                name,
                piece_length,
                pieces,
                files,
                total_length,
                file_tree,
                meta_version,
                private,
            },
            info_hash,
            announce,
            announce_list,
            creation_date,
            comment,
            created_by,
            version,
            raw_info: info_raw,
        })
    }

    pub fn from_info_dict(info_data: &[u8], trackers: &[String]) -> Result<Self, TorrentError> {
        let info_value = oxidebt_bencode::decode(info_data)?;

        let info_dict = info_value.as_dict().ok_or(TorrentError::InvalidFieldType {
            field: "info",
            expected: "dict",
        })?;

        let name = info_dict
            .get(b"name".as_slice())
            .and_then(|v| v.as_str())
            .ok_or(TorrentError::MissingField("name"))?
            .to_string();

        let piece_length = info_dict
            .get(b"piece length".as_slice())
            .and_then(|v| v.as_integer())
            .ok_or(TorrentError::MissingField("piece length"))?;

        if piece_length <= 0 {
            return Err(TorrentError::InvalidPieceLength(piece_length));
        }
        let piece_length = piece_length as u64;

        let pieces = info_dict
            .get(b"pieces".as_slice())
            .and_then(|v| v.as_bytes())
            .cloned();

        if let Some(ref p) = pieces {
            if p.len() % 20 != 0 {
                return Err(TorrentError::InvalidPiecesLength(p.len()));
            }
        }

        let file_tree = info_dict
            .get(b"file tree".as_slice())
            .map(FileTree::from_bencode)
            .transpose()?;

        let meta_version = info_dict
            .get(b"meta version".as_slice())
            .and_then(|v| v.as_integer())
            .map(|v| v as u32);

        let private = info_dict
            .get(b"private".as_slice())
            .and_then(|v| v.as_integer())
            .map(|v| v == 1)
            .unwrap_or(false);

        let (files, total_length) = if let Some(ref ft) = file_tree {
            let files = ft.flatten(std::path::Path::new(&name));
            let total = files.iter().map(|f| f.length).sum();
            (files, total)
        } else if let Some(files_list) = info_dict.get(b"files".as_slice()) {
            parse_v1_files(&name, files_list)?
        } else {
            let length = info_dict
                .get(b"length".as_slice())
                .and_then(|v| v.as_integer())
                .ok_or(TorrentError::MissingField("length"))? as u64;

            let files = vec![File {
                path: PathBuf::from(&name),
                length,
                pieces_root: None,
            }];
            (files, length)
        };

        let version = match (pieces.is_some(), file_tree.is_some()) {
            (true, true) => TorrentVersion::Hybrid,
            (true, false) => TorrentVersion::V1,
            (false, true) => TorrentVersion::V2,
            (false, false) => return Err(TorrentError::MissingField("pieces or file tree")),
        };

        let info_raw = Bytes::copy_from_slice(info_data);

        let info_hash = match version {
            TorrentVersion::V1 => InfoHash::V1(InfoHashV1::from_info_bytes(&info_raw)),
            TorrentVersion::V2 => InfoHash::V2(InfoHashV2::from_info_bytes(&info_raw)),
            TorrentVersion::Hybrid => InfoHash::Hybrid {
                v1: InfoHashV1::from_info_bytes(&info_raw),
                v2: InfoHashV2::from_info_bytes(&info_raw),
            },
        };

        let announce = trackers.first().cloned();
        let announce_list = if trackers.len() > 1 {
            Some(trackers.iter().map(|t| vec![t.clone()]).collect())
        } else {
            None
        };

        Ok(Metainfo {
            info: Info {
                name,
                piece_length,
                pieces,
                files,
                total_length,
                file_tree,
                meta_version,
                private,
            },
            info_hash,
            announce,
            announce_list,
            creation_date: None,
            comment: None,
            created_by: None,
            version,
            raw_info: info_raw,
        })
    }

    pub fn raw_info(&self) -> &[u8] {
        &self.raw_info
    }

    pub fn piece_count(&self) -> usize {
        if let Some(ref pieces) = self.info.pieces {
            pieces.len() / 20
        } else {
            self.info.total_length.div_ceil(self.info.piece_length) as usize
        }
    }

    pub fn piece_hash(&self, index: usize) -> Option<[u8; 20]> {
        self.info.pieces.as_ref().and_then(|pieces| {
            let start = index * 20;
            if start + 20 <= pieces.len() {
                let mut hash = [0u8; 20];
                hash.copy_from_slice(&pieces[start..start + 20]);
                Some(hash)
            } else {
                None
            }
        })
    }

    pub fn piece_length(&self, index: usize) -> u64 {
        let piece_count = self.piece_count();
        if index + 1 < piece_count {
            self.info.piece_length
        } else {
            let remainder = self.info.total_length % self.info.piece_length;
            if remainder == 0 {
                self.info.piece_length
            } else {
                remainder
            }
        }
    }

    pub fn tracker_urls(&self) -> Vec<String> {
        let mut urls = Vec::new();

        if let Some(ref announce) = self.announce {
            urls.push(announce.clone());
        }

        if let Some(ref list) = self.announce_list {
            for tier in list {
                for url in tier {
                    if !urls.contains(url) {
                        urls.push(url.clone());
                    }
                }
            }
        }

        urls
    }

    pub fn is_private(&self) -> bool {
        self.info.private
    }
}

fn parse_v1_files(name: &str, files_value: &Value) -> Result<(Vec<File>, u64), TorrentError> {
    let files_list = files_value
        .as_list()
        .ok_or(TorrentError::InvalidFieldType {
            field: "files",
            expected: "list",
        })?;

    let mut files = Vec::with_capacity(files_list.len());
    let mut total_length = 0u64;

    for file_value in files_list {
        let file_dict = file_value.as_dict().ok_or(TorrentError::InvalidFieldType {
            field: "file",
            expected: "dict",
        })?;

        let length = file_dict
            .get(b"length".as_slice())
            .and_then(|v| v.as_integer())
            .ok_or(TorrentError::MissingField("file length"))? as u64;

        let path_list = file_dict
            .get(b"path".as_slice())
            .and_then(|v| v.as_list())
            .ok_or(TorrentError::MissingField("file path"))?;

        let mut path = PathBuf::from(name);
        for component in path_list {
            let component_str = component.as_str().ok_or(TorrentError::InvalidFilePath(
                "non-string path component".into(),
            ))?;
            path.push(component_str);
        }

        files.push(File {
            path,
            length,
            pieces_root: None,
        });
        total_length += length;
    }

    Ok((files, total_length))
}
