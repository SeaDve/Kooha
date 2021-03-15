<h1 align="center">
	<img src="data/logo/io.github.seadve.Kooha.svg" alt="Kooha" width="192" height="192"/><br>
	Kooha
</h1>

<p align="center"><strong>Simple screen recorder</strong></p>

<p align="center">
  <a href="https://flathub.org/apps/details/io.github.seadve.Kooha"><img width="200" alt="Download on Flathub" src="https://flathub.org/assets/badges/flathub-badge-en.png"/></a>
</p>

<p align="center">
  <img src="screenshots/Kooha-preview.png"/>
</p>

## Description
Kooha is a simple screen recorder built with GTK. It allows you to record your screen and also audio from your microphone or desktop.


## Roadmap

### v 2.0
- [ ] MP4 video format support
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


## Credits

Developed by **[Dave Patrick](https://github.com/SeaDve)**.

Inspired from [RecApp](https://github.com/amikha1lov/RecApp).

The [chime](https://soundbible.com/1598-Electronic-Chime.html) used is under the Public Domain.


## Donate
If you want to support development, consider donating via [PayPal](https://paypal.me/sedve).
