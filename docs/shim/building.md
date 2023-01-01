---
title: Building
editLink: true
---

# {{ $frontmatter.title }}

## Prerequisites

Install [rustup](https://rustup.rs/) if you haven't already.

Then, install the MinGW target for Rust:

```bash
rustup target add x86_64-pc-windows-gnu
```

## Compiling the core module

Clone the LatencyFleX 2 repo:

```bash
git clone https://github.com/ishitatsuyuki/latencyflex2.git
cd latencyflex2
```

Build the module:

```bash
cd core
cargo build --release --target x86_64-pc-windows-gnu
```

The module will be available at `target/x86_64-pc-windows-gnu/release/latencyflex2_rust.dll`.

## Compiling the DXVK fork

Clone the fork and checkout the `lfx2` branch:

```bash
git clone --recursive https://github.com/ishitatsuyuki/dxvk.git -b lfx2
```

Then follow the upstream [build instructions](https://github.com/doitsujin/dxvk#build-instructions).

## Compiling the DXVK-NVAPI fork

Clone the fork and checkout the `lfx2` branch:

```bash
git clone --recursive https://github.com/ishitatsuyuki/dxvk-nvapi.git -b lfx2
```

Then follow the upstream [build instructions](https://github.com/jp7677/dxvk-nvapi#how-to-build).