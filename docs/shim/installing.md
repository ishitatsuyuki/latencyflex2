---
title: Installation
editLink: true
---

# {{ $frontmatter.title }}

## Proton

### Prerequisite

Your Proton installation should be new enough to contain a [fix](https://github.com/ValveSoftware/wine/pull/171) for GPU timestamps. If you're using one of the Proton distributions below, the requirements are:

- **Proton Experimental (Bleeding Edge)**: 7.0-32084-20221229 or later  
- **Proton-GE**: GE-Proton7-44 or later

### Overview

Due to Proton conventions, there are two kind of installation steps:
- Done once per **prefix**: LatencyFleX 2 Core Module
- Done once per **Proton version**: DXVK, DXVK-NVAPI and VKD3D-Proton

### Per-prefix setup

For the following section, set `COMPATDATA` to the path to the app prefix.

This can be determined from the app's Steam AppID, like: 

```bash
APPID=1234567
COMPATDATA=~"/.steam/steam/steamapps/compatdata/$APPID"
```

#### Installing the core module

Copy the just built core module into the `system32` folder under your prefix.

```bash
cp target/x86_64-pc-windows-gnu/release/latencyflex2_rust.dll "$COMPATDATA/pfx/drive_c/windows/system32/"
```

### Per Proton-installation setup

For the following section, set `PROTON_PATH` to the path to Proton installation, like:

```bash
PROTON_PATH=~/.steam/steam/steamapps/common/"Proton - Experimental"
```

#### Installing the DXVK fork

Overwrite your Proton Experimental installation's DXVK dlls with the just built DLLs.

```bash
cp x64/*.dll "$PROTON_PATH/files/lib64/wine/dxvk/"
```

#### Installing the DXVK-NVAPI fork

Overwrite your Proton Experimental installation's DXVK-NVAPI dlls with the just built DLLs.

```bash
cp x64/nvapi64.dll "$PROTON_PATH/files/lib64/wine/nvapi/"
```

#### Installing the VKD3D-Proton fork

Overwrite your Proton Experimental installation's VKD3D-Proton dlls with the just built DLLs.

```bash
cp x64/*.dll "$PROTON_PATH/files/lib64/wine/vkd3d-proton/"
```

Now proceed on to [Environment Variables](#environment-variables) and [Configuration Files](#configuration-files).

## Lutris

### Prerequisite

- Wine upstream: 7.0 or later
- Wine-GE: GE-Proton7-36 or later

For trees based on Proton branches, it is necessary that the [fix](https://github.com/ValveSoftware/wine/pull/171) for GPU timestamps is included.

### Installing the core module

Copy the just built core module into the `system32` folder under your prefix.

```bash
cp target/x86_64-pc-windows-gnu/release/latencyflex2_rust.dll ~/Games/<game>/drive_c/windows/system32/
```

### Installing the DXVK fork

Create a new DXVK runtime for Lutris with the just built artifacts.

```bash
mkdir -p ~/.local/share/lutris/runtime/dxvk/lfx2/
cp -r x64 ~/.local/share/lutris/runtime/dxvk/lfx2/
```

Then **Right Click** the game, go to **Configure** → **Runner Options** → **DXVK version** and manually type in "lfx2".

::: info

If you need the 32-bit version of DXVK, please copy it from another upstream build of DXVK. The DXVK fork will crash on startup if LFX2 cannot be loaded, which is the case for 32-bit since we don't install 32-bit LFX2 modules.

:::

### Installing the DXVK-NVAPI fork

Create a new DXVK-NVAPI runtime for Lutris with the just built artifacts.

```bash
mkdir -p ~/.local/share/lutris/runtime/dxvk-nvapi/lfx2/
cp -r x64 ~/.local/share/lutris/runtime/dxvk-nvapi/lfx2/
```

Then **Right Click** the game, go to **Configure** → **Runner Options** → **DXVK-NVAPI version** and manually type in "lfx2".

### Installing the VKD3D-Proton fork

Create a new VKD3D-Proton runtime for Lutris with the just built artifacts.

```bash
mkdir -p ~/.local/share/lutris/runtime/vkd3d/lfx2/
cp -r x64 ~/.local/share/lutris/runtime/vkd3d/lfx2/
```

Then **Right Click** the game, go to **Configure** → **Runner Options** → **VKD3D version** and manually type in "lfx2".

Now proceed on to [Environment Variables](#environment-variables) and [Configuration Files](#configuration-files).

## Environment Variables

To configure environment variables, using `KEY=value` as an example:
- Steam/Proton: Set `KEY=value %command%` as the game's **launch command line**.
- Lutris: **Right Click** the game, then set in **Configure** → **System Options** → **Environment Variables**. 

### Required

- `PROTON_ENABLE_NVAPI=1` (Proton only): Use this to enable DXVK-NVAPI.
- `DXVK_ENABLE_NVAPI=1` (non-Proton only): Set this to disable DXVK's nvapiHack.
- `DXVK_NVAPI_USE_LATENCY_MARKERS=0`: Set to use no-latency-markers mode (see [Enabling or disabling explicit latency markers](#enabling-or-disabling-explicit-latency-markers))

### Required (Non-NVIDIA GPUs only)

- `DXVK_NVAPI_DRIVER_VERSION=51215`: Override the driver version as one that has Reflex support.
- `DXVK_NVAPI_ALLOW_OTHER_DRIVERS=1`: Enable NVAPI usage with non-NVIDIA GPUs.

### Diagnostics

- `DXVK_NVAPI_LOG_LEVEL=info`: Set this to enable DXVK-NVAPI logging.

## Additional Setup (Non-NVIDIA GPUs only)

### dxvk.conf

Put `dxgi.customVendorId = 10de` in `dxvk.conf` to allow NVAPI usage with non-NVIDIA GPUs.

### `nvngx.dll` Workaround

Some games with DLSS support will hang on launch if NVAPI is spoofed without an NVIDIA driver.
This is caused by the DLSS SDK trying to load `nvngx.dll` and getting stuck if a loop if it does not succeed.

[CyberFSR2](https://www.nexusmods.com/cyberpunk2077/mods/3001?tab=files) contains a vendor-agnostic `nvngx` implementation and can be used to resolve such hangs.

Download the mod from the link, then install with the following instructions (differs from what the mod recommends):

1. Copy `nvngx.dll` to `$COMPATDATA/pfx/drive_c/windows/system32/`.
2. Import `EnableSignatureOverride.reg`. If you have `protontricks` installed, this can be done with:
   ```sh
   protontricks -c "regedit /path/to/EnableSignatureOverride.reg" $APPID
   ```

## Enabling or disabling explicit latency markers

Before LFX2 can work with the game, you need to determine whether the game uses explicit latency markers or not.

Configure LFX2 per the steps above, and include `DXVK_NVAPI_LOG_LEVEL=info` in the environment. Now launch the game, and go to the settings to enable Reflex.

If Reflex was successfully enabled and logging is also working, you should see something like below in the log:

```
NvAPI_D3D_SetSleepMode (Enabled/0us): OK
```

If you don't see this, the configuration might be incorrect.

Next, check whether the following lines exist in the log:

```
NvAPI_D3D_SetLatencyMarker: OK
```

- If the line appears, the game supports latency markers. You do not need to do any additional configuration.
- If the line doesn't appear, the game does not support latency markers. Set `DXVK_NVAPI_USE_LATENCY_MARKERS=0` in the environment and re-launch the game.
