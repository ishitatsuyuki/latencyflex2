---
title: Installation
editLink: true
---

# {{ $frontmatter.title }}

## Proton

### Prerequisite

- **Proton Experimental (Bleeding Edge)**: 7.0-32084-20221229 or later  
  A new version is required due to a [bug](https://github.com/ValveSoftware/wine/pull/171) that resulted in GPU timestamps to have drifted values.

### Installing the core module

Copy the just built core module into the `system32` folder under your prefix.

Replace `<appid>` in the snippet below with your app's Steam AppID.

```bash
cp target/x86_64-pc-windows-gnu/release/latencyflex2_rust.dll ~/.steam/steam/steamapps/compatdata/<appid>/pfx/drive_c/windows/system32/
```

### Installing the DXVK fork

Overwrite your Proton Experimental installation's DXVK dlls with the just built DLLs.

```bash
cp x64/*.dll ~/.steam/steam/steamapps/common/Proton\ -\ Experimental/files/lib64/wine/dxvk
```

### Installing the DXVK-NVAPI fork

Overwrite your Proton Experimental installation's DXVK-NVAPI dlls with the just built DLLs.

```bash
cp x64/nvapi64.dll ~/.steam/steam/steamapps/common/Proton\ -\ Experimental/files/lib64/wine/nvapi
```

Now proceed on to [Environment Variables](#environment-variables) and [Configuration Files](#configuration-files).

## Lutris

### Prerequisite

- Wine upstream: 7.0 or later
- Wine-GE: Wine-GE-Proton7-33 **with binary patching** for GPU timestamp [bug](https://github.com/ValveSoftware/wine/pull/171)  
  Open `lib64/wine/x86_64-unix/winevulkan.so` and replace `be02000000` at offset 0x194a6 with `be01000000`.  
  Caution: The offsets only works for **Wine-GE-Proton7-33**!

### Installing the core module

Copy the just built core module into the `system32` folder under your prefix.

```bash
cp target/x86_64-pc-windows-gnu/release/latencyflex2_rust.dll ~/Games/<game>/drive_c/windows/system32/
```

### Installing the DXVK fork

Create a new DXVK runtime for Lutris with the just built DXVK artifacts.

```bash
mkdir -p ~/.local/share/lutris/runtime/dxvk/lfx2/
cp -r x32 x64 ~/.local/share/lutris/runtime/dxvk/lfx2/
```

Then **Right Click** the game, go to **Configure** → **Runner Options** → **DXVK version** and manually type in "lfx2". 

### Installing the DXVK-NVAPI fork

Create a new DXVK-NVAPI runtime for Lutris with the just built DXVK artifacts.

```bash
mkdir -p ~/.local/share/lutris/runtime/dxvk-nvapi/lfx2/
cp -r x32 x64 ~/.local/share/lutris/runtime/dxvk-nvapi/lfx2/
```

Then **Right Click** the game, go to **Configure** → **Runner Options** → **DXVK-NVAPI version** and manually type in "lfx2".

Now proceed on to [Environment Variables](#environment-variables) and [Configuration Files](#configuration-files).

## Environment Variables

To configure environment variables, using `KEY=value` as an example:
- Steam/Proton: Set `KEY=value %command%` as the game's **launch command line**.
- Lutris: **Right Click** the game, then set in **Configure** → **System Options** → **Environment Variables**. 

### Required

- `PROTON_ENABLE_NVAPI=1` (Proton only): Use this to enable DXVK-NVAPI.
- `DXVK_ENABLE_NVAPI=1` (non-Proton only): Set this to disable DXVK's nvapiHack.

### Required (Non-NVIDIA GPUs only)

- `DXVK_NVAPI_DRIVER_VERSION=49729`: Override the driver version as one that has Reflex support.
- `DXVK_NVAPI_ALLOW_OTHER_DRIVERS`: Enable NVAPI usage with non-NVIDIA GPUs.

### Diagnostics

- `DXVK_NVAPI_LOG_LEVEL=info`: Set this to enable DXVK-NVAPI logging.

## Configuration Files

### Required (Non-NVIDIA GPUs only)

Put `dxgi.customVendorId = 10de` in `dxvk.conf` to allow NVAPI usage with non-NVIDIA GPUs.