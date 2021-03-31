<h1 align="center">
  <img src="data/logo/io.github.seadve.Kooha.svg" alt="Kooha" width="192" height="192"/><br>
  Kooha
</h1>

<p align="center"><strong>Simple screen recorder</strong></p>

<p align="center">
  <a href="https://flathub.org/apps/details/io.github.seadve.Kooha"><img width="200" alt="Download on Flathub" src="https://flathub.org/assets/badges/flathub-badge-en.png"/></a>
</p>

<br>
<p align="center">
  <a href="https://hosted.weblate.org/engage/kooha/">
    <img src="https://hosted.weblate.org/widgets/kooha/-/pot-file/svg-badge.svg" alt="Translation status" />
  </a>
  <a href="https://github.com/SeaDve/Kooha/actions/workflows/testing.yml">
    <img src="https://github.com/SeaDve/Kooha/actions/workflows/testing.yml/badge.svg" alt="CI status"/>
  </a>
  <a href="https://paypal.me/sedve">
    <img src="https://img.shields.io/badge/PayPal-Donate-gray.svg?style=flat&logo=paypal&colorA=0071bb&logoColor=fff" alt="Donate" />
  </a>
</p>

<p align="center">
  <img src="screenshots/Kooha-preview.png" alt="Preview"/>
</p>

## Description
Kooha is a simple screen recorder built with GTK. It allows you to record your screen and also audio from your microphone or desktop.


## Roadmap

### v 2.0
- [ ] MP4 video format support (Already in dev branch)
- [ ] Rewrite VideoRecorder for freedesktop portal
- [ ] Other desktop environment support


## Other Modes of Installation

| Distribution | Package | Maintainer |
|:-:|:-:|:-:|
| Arch Linux (AUR) | [`kooha`](https://aur.archlinux.org/packages/kooha) | [Mark Wagie](https://github.com/yochananmarqos) |


## Building from source

### GNOME Builder (Recommended)
GNOME Builder is the environment used for developing this application. It can use Flatpak manifests to create a consistent building and running environment cross-distro. Thus, it is highly recommended you use it.

1. Download [GNOME Builder](https://flathub.org/apps/details/org.gnome.Builder).
2. In Builder, click the "Clone Repository" button at the bottom, using `https://github.com/SeaDve/Kooha.git` as the URL.
3. Click the build button at the top once the project is loaded.

### Manual with meson
```
git clone https://github.com/SeaDve/Kooha.git
cd Kooha
meson builddir --prefix=/usr/local
ninja -C builddir install
```


## Hidden Configuration Options

### Change frames per second to 60fps

#### Flatpak

The default is 30 fps. Note that using other FPS may cause flickering.

`flatpak run --command=gsettings io.github.seadve.Kooha set io.github.seadve.Kooha video-frames 60`


## Credits

Developed by **[Dave Patrick](https://github.com/SeaDve)** and [contributors](https://github.com/SeaDve/Kooha/graphs/contributors).

Inspired from [RecApp](https://github.com/amikha1lov/RecApp).
