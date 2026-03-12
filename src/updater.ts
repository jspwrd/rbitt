import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export async function checkForUpdates(interactive: boolean = false) {
  try {
    const update = await check();
    if (update) {
      console.log(
        `Update available: ${update.version} (current date: ${update.date})`
      );

      let downloaded = 0;
      let contentLength = 0;

      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            contentLength = event.data.contentLength ?? 0;
            console.log(`Download started, size: ${contentLength}`);
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            console.log(`Downloaded ${downloaded}/${contentLength}`);
            break;
          case "Finished":
            console.log("Download finished");
            break;
        }
      });

      await relaunch();
    } else if (interactive) {
      console.log("No update available");
    }
  } catch (error) {
    console.error("Failed to check for updates:", error);
  }
}
