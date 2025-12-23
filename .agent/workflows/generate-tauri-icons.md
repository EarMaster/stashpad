---
description: Generate Tauri app icons
---

Use the `tauri` command to generate new app icons. Make sure to specify the correct output directory (`./assets/stashpad/tauri/icons` - unless specified otherwise) and a background for the iOS icons (`--ios-color` - choose a fitting color from the color palette). Make sure the path to the icon file is always last.

An example command could be:
`npx tauri icon --output ./assets/stashpad/tauri/icons --ios-color #d8d8d9 ./assets/stashpad/Icon.svg`

Here is a step by step guide:
1. Analyze the icon. Is it large enough, what are the main colors.
2. Choose a background color for the iOS icons from the color palette. Make sure the contrast is good. If you are unsure ask the user to specify a fitting color.
3. Generate the icons based on the command described above.
4. Wait for the icon generation to finish.
5. Make sure `tauri.conf.json` contains the right path to the newly generated icons.