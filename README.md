<h1 align="center">
  <img src="data/icons/io.github.seadve.Kooha.svg" alt="Kooha" width="192" height="192"/>
  <br>
  Kooha
</h1>

<p align="center">
  <strong>Elegantly record your screen</strong>
</p>

<p align="center">
  <a href="https://flathub.org/apps/details/io.github.seadve.Kooha">
    <img width="200" alt="Download on Flathub" src="https://flathub.org/assets/badges/flathub-badge-i-en.svg"/>
  </a>
  <br>
  <a href="https://liberapay.com/SeaDve/donate">
    <img alt="Donate using Liberapay" src="https://liberapay.com/assets/widgets/donate.svg">
  </a>
</p>

<br>

<p align="center">
  <a href="https://hosted.weblate.org/engage/kooha/">
    <img alt="Translation status" src="https://hosted.weblate.org/widgets/kooha/-/pot-file/svg-badge.svg"/>
  </a>
  <a href="https://github.com/SeaDve/Kooha/actions/workflows/ci.yml">
    <img alt="CI status" src="https://github.com/SeaDve/Kooha/actions/workflows/ci.yml/badge.svg"/>
  </a>
  <a href="https://flathub.org/apps/details/io.github.seadve.Kooha">
    <img alt="Flathub downloads" src="https://img.shields.io/badge/dynamic/json?color=informational&label=downloads&logo=flathub&logoColor=white&query=%24.installs_total&url=https%3A%2F%2Fflathub.org%2Fapi%2Fv2%2Fstats%2Fio.github.seadve.Kooha"/>
  </a>
  <a href="https://repology.org/project/kooha/versions">
    <img alt="Packaging status" src="https://repology.org/badge/tiny-repos/kooha.svg">
  </a>
</p>

<p align="center">
  <img src="data/screenshots/preview.png" alt="Preview"/>
</p>

Capture your screen in a intuitive and straightforward way without distractions.

Kooha is a simple screen recorder with a minimal interface. You can simply click
the record button without having to configure a bunch of settings.

The main features of Kooha include the following:
* 🎥 Capture your screen without any hassle.
* 🎙️ Record your microphone, computer sounds, or both at the same time.
* 📼 Support for WebM, MP4, GIF, and MKV formats.
* 🗔 Multiple sources selection.
* 🚀 Optional hardware accelerated encoding
* 🖥️ Select a monitor or window to record.
* 🔲 Create a selection to capture certain area from your screen.
* ⏲️ Set delay to prepare before you start recording.
* 🖱️ Hide or show mouse pointer.
* 💾 Choose a saving location for your recording.
* ⌨️ Utilize helpful keyboard shortcuts.


## 😕 It Doesn't Work

There are many possibilities on why it may not be working. You may not have
the runtime requirements mentioned below installed, or your distro doesn't
support it. For troubleshooting purposes the [screen cast compatibility page](https://github.com/emersion/xdg-desktop-portal-wlr/wiki/Screencast-Compatibility)
of `xdg-desktop-portal-wlr` wiki may be helpful in determining if your distro
has support for it out of the box. If it does, but it still doesn't work, you
can also check for the [troubleshooting checklist](https://github.com/emersion/xdg-desktop-portal-wlr/wiki/%22It-doesn't-work%22-Troubleshooting-Checklist).


## ⚙️ Hidden Configuration Options

### Enable hardware accelerated encoding

Enabling hardware accelerated encoding allows the encoder to utilize GPU for
more efficient or perhaps faster encoding. It is not guaranteed to work on all
devices, so it may give errors such as `no element vaapivp8enc` depending on the
features and capability of your hardware.

To enable all the supported drivers and force Kooha to use VAAPI elements, set
`GST_VAAPI_ALL_DRIVERS` and `KOOHA_VAAPI` both to 1 respectively. These
environment variables are needed for hardware accelerated encoding.

To run Kooha with both set, run the following command:
```shell
GST_VAAPI_ALL_DRIVERS=1 KOOHA_VAAPI=1 flatpak run io.github.seadve.Kooha
```
or if installed locally, run
```shell
GST_VAAPI_ALL_DRIVERS=1 KOOHA_VAAPI=1 kooha
```

### Change frames per second to 60fps

Take note that using other frames per second may cause flickering, depending on
the performance of your device.

You can copy and paste this to the terminal if you installed Kooha as a flatpak:
```shell
flatpak run --command=gsettings io.github.seadve.Kooha set io.github.seadve.Kooha video-framerate 60
```
or if installed locally, run
```shell
gsettings set io.github.seadve.Kooha video-framerate 60
```


## 📋 Runtime Requirements

* pipewire
* gstreamer-plugin-pipewire
* xdg-desktop-portal
* xdg-desktop-portal-(e.g., gtk, kde, wlr)


## 🏗️ Building from source

### GNOME Builder

GNOME Builder is the environment used for developing this application.
It can use Flatpak manifests to create a consistent building and running
environment cross-distro. Thus, it is highly recommended you use it.

1. Download [GNOME Builder](https://flathub.org/apps/details/org.gnome.Builder).
2. In Builder, click the "Clone Repository" button at the bottom, using `https://github.com/SeaDve/Kooha.git` as the URL.
3. Click the build button at the top once the project is loaded.

### Meson

#### Prerequisites

The following packages are required to build Kooha:

* meson
* ninja
* appstream-glib (for checks)
* cargo
* x264 (for MP4)
* gstreamer
* gstreamer-plugins-base
* gstreamer-plugins-ugly (for MP4)
* gstreamer-vaapi (for hardware acceleration)
* glib2
* gtk4
* libadwaita

#### Build Instruction

```shell
git clone https://github.com/SeaDve/Kooha.git
cd Kooha
meson _build --prefix=/usr/local
ninja -C _build install
```


## 🙌 Help translate Kooha

You can help Kooha translate into your native language. If you found any typos
or think you can improve a translation, you can use the [Weblate](https://hosted.weblate.org/engage/kooha/) platform.


## ☕ Support me and the project

Kooha is free and will always be for everyone to use. If you like the project and
would like to support and fund it, you may donate through [Liberapay](https://liberapay.com/SeaDve/).


## 💝 Acknowledgment

[RecApp](https://github.com/amikha1lov/RecApp) greatly inspired the creation of Kooha.
And also, a warm thank you to all the [contributors](https://github.com/SeaDve/Kooha/graphs/contributors)
and [translators](https://hosted.weblate.org/engage/kooha/) from Weblate.
