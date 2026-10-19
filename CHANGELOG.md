# Fork changes
**(19.10.2026, on top of upstream master)**

### Shazam fix
Shazam answers with HTTP 429 and an HTML page after about 20 requests in quick succession.
OneTagger parsed that page as JSON, so every following track failed with
`error decoding response body: expected value at line 1 column 1` (also seen in upstream issues
#441 and #462).
- Shazam requests are throttled globally to one every 3.2 s (about 3 s is sustainable in tests)
- On 429 / server errors all threads pause with exponential backoff and retry instead of failing
- Results are cached per file, so each platform no longer recognizes the same file again
- Responses without a match (no `timestamp` field) are handled as "no match" instead of an error
- Stop interrupts waits for the Shazam rate limit immediately
- Beatport search errors now show the HTTP status. The search page currently answers OneTagger's
  client with a Cloudflare challenge (403); this is reported, not worked around.
  Beatsource has been merged into Beatport and no longer works as a source.

### New: Identify tab
Zero trust identification by audio fingerprint (AcoustID) and Shazam, release choice from
MusicBrainz, automatic tagging and moving of confirmed files, review list for the rest and
lossless undo. See the README for the rules and the reasoning behind them.
- New crate `onetagger-identify` and CLI commands `identify` / `identify-undo`
- Pure Rust Chromaprint fingerprint, fed at the native sample rate like `fpcalc`
  (resampling beforehand changed the fingerprint enough to miss many AcoustID matches)
- Releases of unmerged duplicate recordings on MusicBrainz are evaluated together
- Throttled and disk cached clients for AcoustID (3/s), MusicBrainz (1/s) and Cover Art Archive


### Identify: fewer files in review
Measured on a real review list: about 20 % of the review items are now confirmed, without
loosening the rule that AcoustID and Shazam must agree.
- **Shazam's album on MusicBrainz:** AcoustID only lists releases linked to the exact recording ID
  and MusicBrainz has many unmerged duplicate recordings, so the album Shazam names was often
  missing. It is now looked up on MusicBrainz and its tracklist checked for the song with a
  matching length. If Shazam's album is a compilation that contains the song, the original release
  is kept. If the album only has the song in another length (another edit), that is no objection
  to a plain studio album / EP / single that matches the file length.
- **Live rule:** same title and artist, only Shazam names a live version: AcoustID decides when the
  fingerprint score is at least 0.90 and the length matches the studio recording.
- Artists credited as "feat." in a title count for the artist comparison; a leading "The" in
  titles is ignored.
- Interview / spoken word releases are no longer chosen as the album.
- Review dialog: files only Shazam knows get release suggestions via Shazam's ISRC.

### Identify: duplicates, robustness
- **Duplicates:** if the same recording is already in the library, the better copy stays there
  (lossless > lossy, then bit depth, sample rate, bitrate) and the other one is tagged and moved to
  `<target>/_Duplikate/` with the same folder structure. Both moves are in the journal, undo restores
  everything byte for byte.
- Album comparison ignores ": Original Motion Picture Soundtrack" style suffixes and
  "(The Remixes)"; Shazam albums unknown to MusicBrainz that read like a compilation / DJ series
  ("The Very Best Of", "Italian Hardstyle 14") don't veto an original studio release.
  Measured on a real review sample: another ~15 % of album review cases confirmed.
- FLAC files the default decoder (claxon) rejects are decoded with Symphonia instead.
- Files without a duration in the header get it from the stream.
- Missing write permission (e.g. files owned by another NAS user) is reported clearly and checked
  before anything is changed.

### Identify: versions, paths
- **Version rule:** versions are compared including the remixer ("Yeux Noir remix" vs. "Addal
  remix") instead of being cut off. If Shazam names another version of the same song, the
  version specific fingerprint decides (AcoustID score >= 0.90, length fits) and Shazam's album is
  ignored, because it belongs to Shazam's version. Same for the live rule. If Shazam names no
  version at all, nothing changes.
- Remix recordings prefer the remix single / EP over an album that only has them as bonus track.
- **Fix:** a "/" in a value (AC/DC, "Song A / Song B", dates like 4/22/1982) created extra folders.
  The template is now split into folders before it is filled, so "/" becomes "_".
  `onetagger-cli identify-fix-paths -t <target>` moves files affected by the old behaviour
  (journaled, undo with `identify-undo`).

### Fork maintenance
- Weekly GitHub Action (`.github/workflows/upstream-sync.yml`): updates the upstream status line in
  the README and merges new upstream commits. Conflicting upstream changes are never pushed to
  master automatically; a pull request from the `upstream-master` branch is opened instead.


# 1.7.0
**(03.08.2023)**

### Auto Tag:
- Updated Beatport to support the new site
- Improved match rates
- Selectable overwrite
- Internal refactor to how tags work
- Explicit tag
- BPMSupreme > Latino

### Quick Tag:
- **Multiple files mode**
- Thin view mode
- External player support

### Renamer:
- Fixed input updating
- BPM in MP4

### Other:
- Various bug fixes and improvements in all sections


# 1.6.0
**(14.04.2023)**

### OneTagger:
- Support for WAV (ID3) and OGG files

### Auto Tag:
- **Android version**
- Added Bandcamp
- Added Musixmatch
- Added Deezer
- Lyrics support
- Logging in custom platforms
- Stop button
- Better platform info (supported tags, whether auth is required)
- Profiles (multiple configurations)

### Other
- Many bug fixes in all sections
- Dropped dependency on libsndfile, so compilling 1T should be less painful and more portable


# 1.5.1
**(31.10.2022)**

### OneTagger:
- Older MacOS warning and server version restart

### Auto Tag:
- HOTFIX: Make sure the Move files feature is more safe


# 1.5.0
**(14.10.2022)**

### OneTagger:
- **Rewrite entire frontend to Vue3 + Typescript**

### Auto Tag:
- Only write year
- Track/disc number/total tags
- BPMSupreme, iTunes improvements
- Tag each track using multiple platforms
- Merging styles and genres fixes
- Regex title cleanup
- Move failed/successful files

### Audio Features:
- Spofiy rate limits
- Bug fixes

### Quick Tag:
- List of failed files with reasons


# 1.4.0
**(20.04.2022)**

### OneTagger:
- Added Auto Rename tab (rename your files by to your tags)
- CLI version

### Auto Tag:
- Internal platform system redesign
- Custom platforms support (https://github.com/Marekkon5/onetagger-platform-template/)
- Update Spotify implementation
- Improved match rates, less skipped files
- Various minor platform fixes
- Added reason of failure into status list

### Quick Tag:
- Redesign
- Sorting
- Multiple genres
- File browser
- Subgenres


### Other:
- Windows install to registry
- New logging system
- Split to bunch of different crates
- Minor improvements and bug fixes


# 1.3.0:
**(18.11.2021)**

### Auto Tag:
 - Match by exact ID for Discogs, Beatport
 - Filename template fixes
 - Duration tag
 - `VINYLTRACK` Tag for Discogs
 - Discogs now faster for smaller batches
 - Album artist tag
 - iTunes, Musicbrainz, Beatsource and Spotify support
 - Beatport subgenres, more tags
 - Meta tag
 - Remixer tag
 - Track number tag
 - ISRC tag
 - Shazam to find songs without tag and filename parsing
 - Filter in status page

### Audio Features:
 - Added popularity tag
 - Renamed danceability value to `dance-high, dance-med, dance-low`

### Quick Tag:
 - Internal rewrite, cleaner code, more stable
 - Search and filter

### Tag Editor:
 - CTRL + S
 - Filtering
 - Refactored some code

### Other:
 - General UI improvements
 - Windows: Replace CEF with webview2 - smaller install sizes, more portable.
 - `--expose` command line option to make the server listen on 0.0.0.0
 - Updated dependencies
 - Bug fixes
 - .mp4 Extension support


# 1.2.1:
**(03.07.2021)**

### Auto Tagger
- Fixed bug in 1.2.0 causing Beatport and Traxsource having low match rate

### Other
- Added more info to logs for debugging
- Fixed path pickers not opening with bad path


# 1.2.0
**(02.07.2021)**

### Shared:
- Added M3U playlist support with drag and drop

### Auto Tag:
- Added catalog number, track ID, release ID, version, URL tags
- Added duration matching (WARNING: strict, should be used only in specific situations)
- Tag files without metadata (using filename with custom templates)
- Single page design changes
- Improved matching rates, bug fixes

### Quick Tag:
- Option to load recursively

### Tag Editor:
- Minor design changes

### Other
- Benchmark mode (for testing / debugging purproses, can be ran with `--benchmark` command line argument)
- If you specify path as command line argument, it will be automatically prefilled.



# 1.1.0
**(31.05.2021)**

### Shared:
- Added MP4/M4A support

### Auto Tag:
- Redesign
- Better status page
- Single page setup (enable in settings)
- Camelot key notation
- Juno Download is single thread now
- Bug fixes and improvements related to matching

### Audio Features:
- Fix searching in some edge cases
- Cache Spotify token

### Quick Tag:
- Player UI improvements
- Autosave, Autoplay (can be enabled in settings)
- Bug fixes

## **WARNING: Due to many breaking changes, SETTINGS WILL BE RESET TO DEFAULT. This will hopefully not happen in future again. Sorry for the inconvenience.**



# 1.0.0
**(13.05.2021)**

First public release
