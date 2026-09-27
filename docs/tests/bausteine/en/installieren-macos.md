Download `{{paket}}` from the [preview release page]({{release}}). It is a DMG file.

Open the DMG file and drag the app into the "Applications" folder.

On the first start macOS warns that the app was downloaded from the internet and the developer can't be verified. Open System Settings → Privacy & Security and click **"Open Anyway"** at the bottom, then start the app again. On macOS 14 and older, right-clicking the app and choosing **Open** works as well. On company Macs, IT may have blocked "Open Anyway".

If macOS says the app "is damaged" instead, open a terminal and run:

`xattr -cr "/Applications/{{produkt}}.app"`
