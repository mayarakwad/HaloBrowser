# How to publish a Halo release

A *release* is a version of Halo that people can download from
<https://github.com/mayarakwad/HaloBrowser/releases>. You don't need to build anything
yourself: when you publish a release, GitHub builds Halo and attaches the installer
(`HaloSetup.exe`), a ZIP and a checksum file to it within about 10 minutes.

---

## Before your first release (one time only)

1. Open <https://github.com/mayarakwad/HaloBrowser>.
2. Click **Settings** (top of the page) → **Actions** → **General** in the left menu.
3. Under **Actions permissions**, choose **Allow all actions and reusable workflows**.
4. Scroll to **Workflow permissions**, choose **Read and write permissions**, and click **Save**.

That lets the automatic build attach files to your releases.

---

## Publishing a release (every time)

### Step 1 — Pick the version number

Versions look like `0.1.0`. Raise the **last** number for small fixes (`0.1.1`), the
**middle** one for new features (`0.2.0`), the **first** one for big changes (`1.0.0`).

For the very first release you can keep `0.1.0` and skip to step 2.

To change it:

1. In your repository, click the file **Cargo.toml**.
2. Click the **pencil** icon (Edit this file) at the top right of the file.
3. Change the line `version = "0.1.0"` to the new number, for example `version = "0.2.0"`.
4. Click **Commit changes…**, keep "Commit directly to the main branch", and click **Commit changes**.

### Step 2 — Draft the release

1. On the repository's main page, find **Releases** on the right side and click
   **Create a new release** (or go to <https://github.com/mayarakwad/HaloBrowser/releases/new>).
2. Click **Choose a tag**, type the version with a `v` in front — for example `v0.2.0` —
   and click **Create new tag: v0.2.0 on publish**.
3. Make sure **Target** says `main`.
4. **Release title**: for example `Halo 0.2.0`.
5. **Description**: write what's new in plain words, or click **Generate release notes**.
   You can paste this at the end so people know how to install:

   > **Install:** download **HaloSetup.exe** below and open it.
   > Windows may say "Windows protected your PC" because Halo isn't signed yet —
   > click **More info** → **Run anyway**.

6. Leave **Set as a pre-release** off (turn it on for test versions).

### Step 3 — Publish

1. Click **Publish release**.
2. Click the **Actions** tab. A run called **Release** appears with a yellow dot (working).
3. Wait until the dot turns into a green tick (about 5–10 minutes).
4. Go back to **Releases**. Under **Assets** you'll now see:
   - `HaloSetup.exe` — the installer people download,
   - `Halo-v0.2.0-windows-x64.zip` — the installer and README in a ZIP,
   - `SHA256.txt` — checksums so people can check the download is genuine.

Done — share the link <https://github.com/mayarakwad/HaloBrowser/releases/latest>,
which always points to the newest release.

---

## If something goes wrong

| What you see | What to do |
|---|---|
| The **Release** run has a red ✗ | Click it, then **Re-run jobs** → **Re-run failed jobs**. If it fails again, open the red step and copy the error to your builder. |
| The run says "Resource not accessible by integration" | Do the one-time setup above (Read and write permissions), then re-run. |
| No files appear under Assets | Check the **Actions** tab is enabled (Settings → Actions → General). |
| You published with a typo in the version | Open the release → **Delete**, then delete the tag under **Tags**, and publish again. |

---

## Uploading the installer by hand (without waiting for GitHub)

If you'd rather attach a file you already have (for example `dist\HaloSetup.exe` from the
project folder on your computer):

1. Do steps 1–2 above.
2. Drag `HaloSetup.exe` onto the box **"Attach binaries by dropping them here"**.
3. Click **Publish release**.

The automatic build then also adds its own files; if a file with the same name already
exists, the newest one replaces it.

---

## For people who use the command line

```
git tag v0.2.0
git push origin v0.2.0
```

Pushing a tag that starts with `v` creates the release and attaches the files automatically.
