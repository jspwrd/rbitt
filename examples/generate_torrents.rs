use bytes::Bytes;
use oxidebt_bencode::{encode, Value};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn main() {
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples")
        .join("torrents");

    fs::create_dir_all(&examples_dir).unwrap();

    generate_v1_single_file(&examples_dir);

    generate_v1_multi_file(&examples_dir);

    generate_v2_torrent(&examples_dir);

    generate_hybrid_torrent(&examples_dir);

    println!("Generated example torrents in {:?}", examples_dir);
}

fn generate_v1_single_file(dir: &Path) {
    let piece_length: i64 = 16384;
    let file_length: i64 = 1024;

    let piece_data = vec![0u8; file_length as usize];
    let mut hasher = Sha1::new();
    hasher.update(&piece_data);
    let piece_hash: [u8; 20] = hasher.finalize().into();

    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"length"), Value::Integer(file_length));
    info.insert(
        Bytes::from_static(b"name"),
        Value::Bytes(Bytes::from_static(b"example_v1.txt")),
    );
    info.insert(
        Bytes::from_static(b"piece length"),
        Value::Integer(piece_length),
    );
    info.insert(
        Bytes::from_static(b"pieces"),
        Value::Bytes(Bytes::from(piece_hash.to_vec())),
    );

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::Bytes(Bytes::from_static(
            b"udp://tracker.example.com:6969/announce",
        )),
    );
    torrent.insert(
        Bytes::from_static(b"comment"),
        Value::Bytes(Bytes::from_static(
            b"Example BitTorrent v1 single-file torrent",
        )),
    );
    torrent.insert(
        Bytes::from_static(b"created by"),
        Value::Bytes(Bytes::from_static(b"RBitt/0.1.0")),
    );
    torrent.insert(
        Bytes::from_static(b"creation date"),
        Value::Integer(1732752000),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    let encoded = encode(&Value::Dict(torrent)).unwrap();
    fs::write(dir.join("example_v1_single.torrent"), &encoded).unwrap();
    println!("Created: example_v1_single.torrent");
}

fn generate_v1_multi_file(dir: &Path) {
    let piece_length: i64 = 16384;

    let files = [
        (b"file1.txt".to_vec(), 512i64),
        (b"file2.txt".to_vec(), 768i64),
        (b"subdir/file3.txt".to_vec(), 256i64),
    ];

    let total_length: i64 = files.iter().map(|(_, len)| len).sum();

    let piece_data = vec![0u8; total_length as usize];
    let mut hasher = Sha1::new();
    hasher.update(&piece_data);
    let piece_hash: [u8; 20] = hasher.finalize().into();

    let file_list: Vec<Value> = files
        .iter()
        .map(|(path, length)| {
            let mut file_dict = BTreeMap::new();
            file_dict.insert(Bytes::from_static(b"length"), Value::Integer(*length));

            let path_str = String::from_utf8_lossy(path);
            let path_parts: Vec<Value> = path_str
                .split('/')
                .map(|p| Value::Bytes(Bytes::from(p.as_bytes().to_vec())))
                .collect();
            file_dict.insert(Bytes::from_static(b"path"), Value::List(path_parts));

            Value::Dict(file_dict)
        })
        .collect();

    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"files"), Value::List(file_list));
    info.insert(
        Bytes::from_static(b"name"),
        Value::Bytes(Bytes::from_static(b"example_v1_multifile")),
    );
    info.insert(
        Bytes::from_static(b"piece length"),
        Value::Integer(piece_length),
    );
    info.insert(
        Bytes::from_static(b"pieces"),
        Value::Bytes(Bytes::from(piece_hash.to_vec())),
    );

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::Bytes(Bytes::from_static(
            b"udp://tracker.example.com:6969/announce",
        )),
    );
    torrent.insert(
        Bytes::from_static(b"announce-list"),
        Value::List(vec![
            Value::List(vec![Value::Bytes(Bytes::from_static(
                b"udp://tracker.example.com:6969/announce",
            ))]),
            Value::List(vec![Value::Bytes(Bytes::from_static(
                b"http://backup.tracker.example.com/announce",
            ))]),
        ]),
    );
    torrent.insert(
        Bytes::from_static(b"comment"),
        Value::Bytes(Bytes::from_static(
            b"Example BitTorrent v1 multi-file torrent",
        )),
    );
    torrent.insert(
        Bytes::from_static(b"created by"),
        Value::Bytes(Bytes::from_static(b"RBitt/0.1.0")),
    );
    torrent.insert(
        Bytes::from_static(b"creation date"),
        Value::Integer(1732752000),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    let encoded = encode(&Value::Dict(torrent)).unwrap();
    fs::write(dir.join("example_v1_multi.torrent"), &encoded).unwrap();
    println!("Created: example_v1_multi.torrent");
}

fn generate_v2_torrent(dir: &Path) {
    let piece_length: i64 = 16384;
    let file_length: i64 = 1024;

    let piece_data = vec![0u8; file_length as usize];
    let mut hasher = Sha256::new();
    hasher.update(&piece_data);
    let pieces_root: [u8; 32] = hasher.finalize().into();

    let mut file_entry = BTreeMap::new();
    file_entry.insert(Bytes::from_static(b""), {
        let mut attrs = BTreeMap::new();
        attrs.insert(Bytes::from_static(b"length"), Value::Integer(file_length));
        attrs.insert(
            Bytes::from_static(b"pieces root"),
            Value::Bytes(Bytes::from(pieces_root.to_vec())),
        );
        Value::Dict(attrs)
    });

    let mut file_tree = BTreeMap::new();
    file_tree.insert(
        Bytes::from_static(b"example_v2.txt"),
        Value::Dict(file_entry),
    );

    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"file tree"), Value::Dict(file_tree));
    info.insert(Bytes::from_static(b"meta version"), Value::Integer(2));
    info.insert(
        Bytes::from_static(b"name"),
        Value::Bytes(Bytes::from_static(b"example_v2.txt")),
    );
    info.insert(
        Bytes::from_static(b"piece length"),
        Value::Integer(piece_length),
    );

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::Bytes(Bytes::from_static(
            b"udp://tracker.example.com:6969/announce",
        )),
    );
    torrent.insert(
        Bytes::from_static(b"comment"),
        Value::Bytes(Bytes::from_static(
            b"Example BitTorrent v2 torrent (BEP 52)",
        )),
    );
    torrent.insert(
        Bytes::from_static(b"created by"),
        Value::Bytes(Bytes::from_static(b"RBitt/0.1.0")),
    );
    torrent.insert(
        Bytes::from_static(b"creation date"),
        Value::Integer(1732752000),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    let encoded = encode(&Value::Dict(torrent)).unwrap();
    fs::write(dir.join("example_v2.torrent"), &encoded).unwrap();
    println!("Created: example_v2.torrent");
}

fn generate_hybrid_torrent(dir: &Path) {
    let piece_length: i64 = 16384;
    let file_length: i64 = 1024;

    let piece_data = vec![0u8; file_length as usize];
    let mut sha1_hasher = Sha1::new();
    sha1_hasher.update(&piece_data);
    let v1_piece_hash: [u8; 20] = sha1_hasher.finalize().into();

    let mut sha256_hasher = Sha256::new();
    sha256_hasher.update(&piece_data);
    let pieces_root: [u8; 32] = sha256_hasher.finalize().into();

    let mut file_entry = BTreeMap::new();
    file_entry.insert(Bytes::from_static(b""), {
        let mut attrs = BTreeMap::new();
        attrs.insert(Bytes::from_static(b"length"), Value::Integer(file_length));
        attrs.insert(
            Bytes::from_static(b"pieces root"),
            Value::Bytes(Bytes::from(pieces_root.to_vec())),
        );
        Value::Dict(attrs)
    });

    let mut file_tree = BTreeMap::new();
    file_tree.insert(
        Bytes::from_static(b"example_hybrid.txt"),
        Value::Dict(file_entry),
    );

    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"length"), Value::Integer(file_length));
    info.insert(
        Bytes::from_static(b"name"),
        Value::Bytes(Bytes::from_static(b"example_hybrid.txt")),
    );
    info.insert(
        Bytes::from_static(b"piece length"),
        Value::Integer(piece_length),
    );
    info.insert(
        Bytes::from_static(b"pieces"),
        Value::Bytes(Bytes::from(v1_piece_hash.to_vec())),
    );
    info.insert(Bytes::from_static(b"file tree"), Value::Dict(file_tree));
    info.insert(Bytes::from_static(b"meta version"), Value::Integer(2));

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::Bytes(Bytes::from_static(
            b"udp://tracker.example.com:6969/announce",
        )),
    );
    torrent.insert(
        Bytes::from_static(b"comment"),
        Value::Bytes(Bytes::from_static(b"Example hybrid v1+v2 torrent (BEP 47)")),
    );
    torrent.insert(
        Bytes::from_static(b"created by"),
        Value::Bytes(Bytes::from_static(b"RBitt/0.1.0")),
    );
    torrent.insert(
        Bytes::from_static(b"creation date"),
        Value::Integer(1732752000),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    let encoded = encode(&Value::Dict(torrent)).unwrap();
    fs::write(dir.join("example_hybrid.torrent"), &encoded).unwrap();
    println!("Created: example_hybrid.torrent");
}
