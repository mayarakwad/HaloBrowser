# Google sign-in file

Put the file you download from Google Cloud here, for example
`google/client_secret_1234-abcd.apps.googleusercontent.com.json`.
Any file name ending in `.json` works. The next build reads it automatically and turns on
**Continue with Google** in the setup wizard and in Settings → Account.

This folder's `.json` files are **never uploaded to GitHub** (they're in `.gitignore`).

## Getting the file from Google (one time)

Google now keeps these settings in **Google Auth Platform** (the old "OAuth consent screen"
page sends you there). Its menu on the left has **Overview, Branding, Audience, Clients and
Data access**.

1. Go to <https://console.cloud.google.com/auth/overview> and pick your project (top bar).
   If it says the platform isn't configured yet, click **Get started**: app name "Halo",
   your support email, **Audience: External**, your contact email, agree, **Create**.
2. **Branding**: check the app name and support email (a logo is optional), **Save**.
3. **Audience**: this is where the old consent screen settings went.
   - While *Publishing status* is **Testing**, only the accounts under **Test users** can sign
     in: click **Add users**, type each Google email, **Save** (up to 100).
   - To let anyone sign in, click **Publish app** → **Confirm**. Halo only asks for name,
     email and picture, so Google doesn't need to review it.
4. **Data access** (optional): **Add or remove scopes**, tick `openid`, `.../auth/userinfo.email`
   and `.../auth/userinfo.profile`, **Update**, **Save**. These basic scopes work even if you skip this.
5. **Clients → Create client**: **Application type: Desktop app**, name "Halo", **Create**.
6. **Download JSON** in the window that appears (or later from the download icon next to the
   client in **Clients**) and put that file in this folder.

Direct links: [Audience](https://console.cloud.google.com/auth/audience) ·
[Clients](https://console.cloud.google.com/auth/clients) ·
[Data access](https://console.cloud.google.com/auth/scopes)

No redirect address needs to be entered: Halo signs in through your web browser and
receives the answer on a one-time local address (`http://127.0.0.1:<port>`), which Google
allows for Desktop app clients.

## For GitHub's automatic builds

So the installers built on GitHub also have Google sign-in, without the file being public:

1. Open the repository → **Settings → Secrets and variables → Actions → New repository secret**.
2. Name: `GOOGLE_CLIENT_JSON`. Value: paste the **whole contents** of the JSON file.
3. Click **Add secret**. The next build or release picks it up.
