# Kidex

A simple file indexing service

## Installation

On Arch or Arch-based distros the AUR package [kidex-git](https://aur.archlinux.org/packages/kidex-git) can be installed.

On nixos:

1. You'll need to add this repo into your flake.nix

```nix
{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    kidex.url = "github:ReStranger/kidex?ref=nix";
  };

  outputs = {
    self,
    nixpkgs,
    kidex,
    ...
  }: {
    ...
  };
}
```

2. After that, add package into your environment.systemPackages or home.packages:

```nix
# Nixos configuraion
{
  pkgs,
  inputs,
  ...
}: {
  environment.systemPackages = with pkgs; [
    inputs.kidex.packages.${pkgs.system}.default
  ];
}
```

```nix
# Home-manager configuration
{
  pkgs,
  inputs,
  ...
}: {
  home.packages = with pkgs; [
    inputs.kidex.packages.${pkgs.system}.default
  ];
}
```

### Manual installation

Simply run the following in the projects directory.

```sh
cargo install --path .
```

## Configuration

Kidex only has a single config file to be placed in `~/.config/kidex.ron`, which uses the following structure:

```ron
Config(
  ignored: [], // A list of patterns to be ignored in all directories
  directories: [
    WatchDir(
      path: "/home/kirottu/Documents", // The root folder to be indexed
      recurse: true, // Recursively index and watch all subfolders
      ignored: [], // Ignore patterns specifically for this directory
    ),
  ],
)
```

## Usage

To start the service, simply run `kidex` and make sure it runs in the background. To get data from the service,
the provided `kidex-client` binary can be used to get JSON output of the index. Alternatively a tool like [Anyrun](https://github.com/Kirottu/anyrun)
(with the kidex plugin) can be used to search for files using kidex.
