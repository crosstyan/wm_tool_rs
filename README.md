# Rust implementation of [wm_tool](https://github.com/crosstyan/wm_tool)

Why? RIIR (Rewrite it in Rust) is awesome!

Jokes aside, the original [C implementation](https://github.com/IOsetting/wm-sdk-w806/blob/main/tools/W806/wm_tool.c) is kinda messy.
And it's the byproduct of [making a usable firmware for the W806](https://github.com/crosstyan/w806-air103-cmake)
since I need to verify if the firmware flashing process is correct.

## Usage

```bash
wm_tool_rs --port COM8 --image-path "C:\Users\cross\Desktop\code\wm806-cmake\build\demo.fls"
```

Only plan to support RTS(Request To Send) for resetting for now.
See more on [联盛德 HLK-W806 (三): 免按键自动下载和复位](https://www.cnblogs.com/milton/p/15609031.html)
and [联盛德 HLK-W806 (七): 兼容开发板 LuatOS Air103](https://www.cnblogs.com/milton/p/15676414.html)

## TODO

- [x] Firmware Download
- [ ] AT reset/Manual reset
- [ ] [Firmware Patching](https://github.com/IOsetting/wm-sdk-w806/blob/03b0f7fec247b05e16b5abb8c2310958f07114e9/tools/W806/wm_tool.c#L3606-L3717)
- [ ] Firmware Encryption
- [ ] Firmware Signing
- [ ] Firmware Compression

I'm currently not using the Encryption, Signing, and Compression features.
Try to keep it simple and clean.
