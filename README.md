<p align='center'>
    <img alt='Logo' src='https://raw.githubusercontent.com/Marekkon5/onetagger/master/assets/onetagger-logo-github.png'>
</p>
<h1 align='center'>The ultimate cross-platform tagger for DJs</h1>

<h3 align='center'><b>
<a href='https://onetagger.github.io/'>Website</a> | <a href='https://github.com/Marekkon5/onetagger/releases/'>Latest Release</a>
</b></h3>
<br>

<p align='center'>
    <img alt='Version Badge' src='https://img.shields.io/github/v/release/marekkon5/onetagger?label=Latest%20Release'>
    <img alt='Supported OS' src='https://img.shields.io/badge/OS-Windows%2C%20Mac%20OS%2C%20Linux-orange'>
    <img alt='Build Status' src='https://img.shields.io/github/actions/workflow/status/marekkon5/onetagger/build.yml?branch=master'>
</p>

<h3 align='center'><b></b></h3>
<hr>

Cross-platform music tagger.
It can fetch metadata from Beatport, Traxsource, Juno Download, Discogs, Musicbrainz and Spotify.
It is also able to fetch Spotify's Audio Features based on ISRC & exact match. 
There is a manual tag editor and quick tag editor which lets you use keyboard shortcuts. Written in Rust, Vue.js and Quasar.

MP3, AIFF, FLAC, M4A (AAC, ALAC) supported.

*For more info and tutorials check out our [website](https://onetagger.github.io/).*

> **About this fork**
> This is a fork of [Marekkon5/onetagger](https://github.com/Marekkon5/onetagger) with two additions:
> a fix for Shazam recognition stopping after ~20 tracks, and a new **Identify** tab that
> identifies unknown files by audio only (AcoustID fingerprint + Shazam must agree) and tags them
> from MusicBrainz. See [Identify](#identify-fork) below and the [changelog](CHANGELOG.md).

https://user-images.githubusercontent.com/15169286/193469224-cbf3af71-f6d7-4ecd-bdbf-5a1dca2d99c8.mp4


## Installing

You can download latest binaries from [releases](https://github.com/Marekkon5/onetagger/releases)


## Identify (fork)

Built for collections where the existing tags and filenames can't be trusted (e.g. downloads an
importer rejected as uncertain). Other taggers usually recognize the *song* correctly but then pick
an arbitrary *release* (a compilation, another live album) and overwrite good data with it.
Identify works on a **zero trust** basis instead:

- **Two independent opinions.** The file is fingerprinted with Chromaprint and looked up on
  [AcoustID](https://acoustid.org); in parallel it is recognized by Shazam. A file only counts as
  identified when both name the same recording (same ISRC, or matching title/artist with compatible
  version such as live/remix) and the fingerprint score and duration fit.
- **Existing tags are only claims.** Old tags and the filename are shown for comparison but never
  used as evidence.
- **Release choice.** Only releases that contain exactly the confirmed recording are candidates
  (a remaster is its own recording on MusicBrainz). Shazam naming the album, other confirmed files
  of the same run on that album and a matching duration count as evidence. Without evidence the
  earliest official studio album wins; compilations are only used when Shazam names them. If Shazam
  names a different album than the one chosen, the file goes to review.
- **Confirmed files** are tagged from MusicBrainz (Picard compatible tags incl. MusicBrainz IDs and
  cover from the Cover Art Archive; old tags are removed except ReplayGain/encoder) and moved into
  the target folder using a template, e.g. `%albumartist%/%album% (%year%)/%track% - %title%`.
- **Everything else** lands in a review list that shows Shazam, AcoustID, the old tags and the
  release candidates side by side, with playback, "apply with this release" and "skip".
- **Undo.** Before a file is touched its raw metadata is saved in a journal inside the target folder
  (`.onetagger-identify/<timestamp>/`). Undo moves the files back and restores the metadata byte for
  byte (MP3/FLAC metadata blocks, full MP4 item list).
- **Long runs.** Shazam allows roughly one request every 3 seconds, so large folders take hours.
  The run continues in the background while you review, results are kept on disk, and Shazam /
  AcoustID / MusicBrainz responses are cached so a restarted run catches up quickly. Stop takes
  effect within seconds.

**Requirements:** a free AcoustID application key (acoustid.org → *Register your application*),
entered in the Identify tab.

**CLI:**
```
onetagger-cli identify -p <folder or file list> --acoustid-key <key> [--target <folder> --template <template>]
onetagger-cli identify-undo -j <target>/.onetagger-identify/<timestamp>
```
Without `--target` it is a dry run that only writes a JSON report.

## Credits
Bas Curtiz - UI, Idea, Help  
SongRec (Shazam support) - https://github.com/marin-m/SongRec  
Chromaprint (fingerprint, via [rusty-chromaprint](https://crates.io/crates/rusty-chromaprint)), [AcoustID](https://acoustid.org), [MusicBrainz](https://musicbrainz.org) and the [Cover Art Archive](https://coverartarchive.org) - Identify

## Support
You can support the original OneTagger project by donating on [PayPal](https://paypal.me/marekkon5) or [Patreon](https://www.patreon.com/onetagger).

**Note:** these donations go to the original author of OneTagger (Marekkon5), not to this fork.
<!-- upstream-status -->Last upstream release: 1.7.0 (2023-08-03), last upstream commit: 2026-02-13.<!-- /upstream-status -->

## Compilling

### Linux & Mac
Install dependencies: [rustup](https://rustup.rs), [node](https://nodejs.org/en/download/package-manager/), [pnpm](https://pnpm.io/installation)

**Install remaining dependencies**
```
sudo apt install -y lld autogen libasound2-dev pkg-config make libssl-dev gcc g++ curl wget git libwebkit2gtk-4.1-dev
```

**Compile UI**
```
cd client
pnpm i
pnpm run build
cd ..
```

**Compile**
```
cargo build --release
```
Output will be in: `target/release/onetagger`


### Windows
You need to install dependencies: [rustup](https://rustup.rs), [nodejs](https://nodejs.org/en/download/), [Visual Studio 2019 Build Tools](https://aka.ms/vs/16/release/vs_buildtools.exe), [pnpm](https://pnpm.io/installation)

**Compile UI:**
```
cd client
pnpm i
pnpm run build
cd ..
```

**Compile OneTagger:**
```
cargo build --release
```

Output will be inside `target\release` folder.
