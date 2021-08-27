<h1 align="center">
  <img src="data/icons/io.github.seadve.Kooha.svg" alt="Kooha" width="192" height="192"/><br>
  Kooha
</h1>

<p align="center"><strong>Elegantly record your screen</strong></p>

<p align="center">
  <a href="https://flathub.org/apps/details/io.github.seadve.Kooha">
    <img width="200" src="https://flathub.org/assets/badges/flathub-badge-en.png" alt="Download on Flathub">
  </a>
  <br>
  <a href="https://liberapay.com/SeaDve/donate">
    <img src="https://liberapay.com/assets/widgets/donate.svg" alt="Donate using Liberapay">
  </a>
</p>

<br>
<p align="center">
  <a href="https://hosted.weblate.org/engage/kooha/">
    <img src="https://hosted.weblate.org/widgets/kooha/-/pot-file/svg-badge.svg" alt="Translation status" />
  </a>
  <a href="https://github.com/SeaDve/Kooha/actions/workflows/ci.yml">
    <img src="https://github.com/SeaDve/Kooha/actions/workflows/ci.yml/badge.svg" alt="CI status"/>
  </a>
  <a href="https://repology.org/project/kooha/versions">
    <img src="https://repology.org/badge/tiny-repos/kooha.svg" alt="Packaging status">
  </a>
</p>

<p align="center">
  <img src="data/screenshots/preview.png" alt="Preview"/>
</p>

Capture your screen in a straightforward and painless way without distractions.

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
has support for it out of the box. If your distro supports it, but it still
doesn't work, you can also check for the [troubleshooting checklist](https://github.com/emersion/xdg-desktop-portal-wlr/wiki/%22It-doesn't-work%22-Troubleshooting-Checklist).


## ⚙️ Hidden Configuration Options

### Enable hardware accelerated encoding

Enabling hardware accelerated encoding allows the encoder to utilize GPU for
more efficient or perhaps faster encoding. It may give errors such as
`no element vaapivp8enc` depending on the featues and capability of your 
hardware, so it is not guaranteed to work on all devices.

To enable all the supported drivers, set `GST_VAAPI_ALL_DRIVERS` to 1, and to
set Kooha to use vaapi elements, also set `KOOHA_VAAPI` to 1. Both are needed
to be set to enable hardware accelerated encoding.

To run Kooha with both enabled, run the following command:
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
flatpak run --command=gsettings io.github.seadve.Kooha set io.github.seadve.Kooha video-frames 60
```
or if installed locally, run
```
gsettings set io.github.seadve.Kooha video-frames 60
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
The following packages are required to build Kooha. Note that these are not
needed to be installed on your device when you use GNOME Builder because it
takes care of the required dependencies.

* meson
* ninja
* appstream-glib
* python3
* python3-gobject
* gstreamer
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
