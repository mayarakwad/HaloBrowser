# Google sign-in file

Put the file you download from Google Cloud here, for example
`google/client_secret_1234-abcd.apps.googleusercontent.com.json`.
Any file name ending in `.json` works. The next build reads it automatically and turns on
**Continue with Google** in the setup wizard and in Settings → Account.

This folder's `.json` files are **never uploaded to GitHub** (they're in `.gitignore`).

## Getting the file from Google (one time)

1. Go to <https://console.cloud.google.com/> and create a project (for example "Halo").
2. **APIs & Services → OAuth consent screen**: choose **External**, give the app name
   "Halo", your support email, and add the scopes **openid**, **email** and **profile**.
   While the app is in *Testing*, add the Google accounts that may sign in under **Test users**
   (or press **Publish app** so anyone can sign in).
3. **APIs & Services → Credentials → Create credentials → OAuth client ID**.
4. **Application type: Desktop app**, name "Halo", then **Create**.
5. Click **Download JSON** and put that file in this folder.

No redirect address needs to be entered: Halo signs in through your web browser and
receives the answer on a one-time local address (`http://127.0.0.1:<port>`), which Google
allows for Desktop app clients.

## For GitHub's automatic builds

So the installers built on GitHub also have Google sign-in, without the file being public:

1. Open the repository → **Settings → Secrets and variables → Actions → New repository secret**.
2. Name: `GOOGLE_CLIENT_JSON`. Value: paste the **whole contents** of the JSON file.
3. Click **Add secret**. The next build or release picks it up.
