//! Finishing a downloaded video with ffmpeg: joining separate sound in, adding subtitles and
//! a cover picture, or turning sound saved on its own into MP3. One pass makes the final
//! file, so it's only written once. What a file can't hold goes beside it, so nothing the
//! user asked for is lost.

use std::ffi::OsString;
use std::fs;
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Keeps ffmpeg from flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// How long a file that was just read may stay locked, as it can be while a virus scanner
/// looks at it.
const DELETE_ATTEMPTS: u32 = 30;
const DELETE_WAIT: Duration = Duration::from_millis(100);

/// What's left to do to a downloaded video, saved with the download so it survives a
/// restart. Subtitles and the cover were fetched when the video was added, into
/// Tributary's own folder, because their addresses can expire.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FinishPlan {
    /// Turns sound saved on its own into MP3.
    pub mp3: bool,
    /// The download is sound with no picture.
    pub audio_only: bool,
    pub subtitles: Vec<PlannedSubtitle>,
    pub cover: Option<PathBuf>,
    /// Written into the file, so players show it.
    pub title: Option<String>,
    /// Only this part of it is kept.
    pub section: Option<Section>,
}

/// The part of a video to keep, in seconds from its start.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub from: f64,
    pub to: f64,
    /// Cut at exactly these times, which means encoding the picture again. Otherwise the
    /// cut is at the nearest keyframe before `from`, which is fast and loses nothing.
    pub exact: bool,
}

impl Section {
    /// Whether this makes sense for something `seconds` long, or of unknown length.
    pub fn fits(&self, seconds: f64) -> bool {
        self.from.is_finite()
            && self.to.is_finite()
            && self.from >= 0.0
            && self.to > self.from
            && (seconds <= 0.0 || self.from < seconds)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedSubtitle {
    pub path: PathBuf,
    /// Such as `en`.
    pub language: String,
    /// Such as `English`.
    pub name: String,
}

impl FinishPlan {
    pub fn read(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }

    pub fn write(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        !self.mp3 && self.subtitles.is_empty() && self.cover.is_none() && self.section.is_none()
    }

    /// What the list says while this is done.
    pub fn label(&self) -> &'static str {
        if self.mp3 {
            en!("Converting to MP3…")
        } else if self.section.is_some() {
            en!("Cutting the part chosen…")
        } else if !self.subtitles.is_empty() {
            en!("Adding subtitles…")
        } else {
            en!("Adding the cover…")
        }
    }

    /// The files fetched for this plan, which are Tributary's to tidy away.
    pub fn fetched_files(&self) -> impl Iterator<Item = &Path> {
        self.subtitles
            .iter()
            .map(|subtitle| subtitle.path.as_path())
            .chain(self.cover.as_deref())
    }
}

/// The finished file.
#[derive(Debug)]
pub struct Finished {
    pub path: PathBuf,
    pub size: u64,
    /// Set when the separate sound file couldn't be deleted afterwards.
    pub leftover: Option<String>,
}

/// Which of the extras a kind of file can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Room {
    subtitles: bool,
    cover: bool,
}

fn room_in(container: &str) -> Room {
    match container {
        "mp4" | "m4v" | "mov" | "mkv" => Room {
            subtitles: true,
            cover: true,
        },
        "webm" => Room {
            subtitles: true,
            cover: false,
        },
        "mp3" | "m4a" => Room {
            subtitles: false,
            cover: true,
        },
        _ => Room {
            subtitles: false,
            cover: false,
        },
    }
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// What a subtitle becomes inside a kind of file.
fn subtitle_codec(container: &str, source: &Path) -> &'static str {
    match container {
        "mkv" if extension_of(source) == "ass" => "ass",
        "mkv" => "srt",
        "webm" => "webvtt",
        _ => "mov_text",
    }
}

fn picture_mime(path: &Path) -> &'static str {
    match extension_of(path).as_str() {
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/jpeg",
    }
}

/// ffmpeg's arguments for making `output` from `video`, joining in `sound` if it's separate,
/// and putting in the plan's subtitles and cover where `output` has room for them. The
/// picture and sound are copied as they are; only MP3 is encoded.
fn arguments(
    video: &Path,
    sound: Option<&Path>,
    plan: &FinishPlan,
    has_picture: bool,
    output: &Path,
) -> Vec<OsString> {
    let container = extension_of(output);
    let room = room_in(&container);
    let mp3 = container == "mp3";
    let subtitles: &[PlannedSubtitle] = if room.subtitles { &plan.subtitles } else { &[] };
    let cover = plan.cover.as_deref().filter(|_| room.cover);
    // Matroska keeps a cover as an attachment rather than as a picture stream.
    let cover_as_attachment = container == "mkv";

    let mut arguments: Vec<OsString> = vec!["-y".into(), "-loglevel".into(), "error".into()];
    let mut inputs = 0;
    let section = plan.section;
    let mut input = |arguments: &mut Vec<OsString>, path: &Path, timed: bool| {
        // Seeking each timed input keeps picture, sound and subtitles together.
        if let (Some(section), true) = (section, timed) {
            arguments.extend([
                "-ss".into(),
                format!("{:.3}", section.from).into(),
                "-to".into(),
                format!("{:.3}", section.to).into(),
            ]);
        }
        arguments.push("-i".into());
        arguments.push(path.into());
        inputs += 1;
        inputs - 1
    };
    fn push(arguments: &mut Vec<OsString>, items: &[&str]) {
        arguments.extend(items.iter().map(OsString::from));
    }

    input(&mut arguments, video, true);
    let sound_input = sound.map(|sound| input(&mut arguments, sound, true));
    let subtitle_inputs: Vec<usize> = subtitles
        .iter()
        .map(|subtitle| input(&mut arguments, &subtitle.path, true))
        .collect();
    let cover_input = cover
        .filter(|_| !cover_as_attachment)
        .map(|cover| input(&mut arguments, cover, false));

    // What goes in, in order: the picture, the sound, the subtitles, then the cover.
    if mp3 {
        push(&mut arguments, &["-map", "0:a:0"]);
    } else {
        if has_picture {
            push(&mut arguments, &["-map", "0:v:0"]);
        }
        match sound_input {
            Some(index) => push(&mut arguments, &["-map", &format!("{index}:a:0")]),
            None => push(&mut arguments, &["-map", "0:a?"]),
        }
    }
    for index in &subtitle_inputs {
        push(&mut arguments, &["-map", &format!("{index}:0")]);
    }
    if let Some(index) = cover_input {
        push(&mut arguments, &["-map", &format!("{index}:0")]);
    }

    // How each is written.
    if mp3 {
        push(&mut arguments, &["-c:a", "libmp3lame", "-q:a", "2"]);
    } else {
        push(&mut arguments, &["-c", "copy"]);
        // An exact cut encodes the picture again, since a copy can only start at a
        // keyframe. WebM can't hold H.264, so it keeps the keyframe cut.
        let exact = section.is_some_and(|section| section.exact);
        if exact && has_picture && container != "webm" {
            push(
                &mut arguments,
                &[
                    "-c:v:0", "libx264", "-preset", "veryfast", "-crf", "18", "-c:a", "aac",
                    "-b:a", "192k",
                ],
            );
        }
    }
    for (index, subtitle) in subtitles.iter().enumerate() {
        push(
            &mut arguments,
            &[
                &format!("-c:s:{index}"),
                subtitle_codec(&container, &subtitle.path),
                &format!("-metadata:s:s:{index}"),
                &format!("language={}", subtitle.language),
                &format!("-metadata:s:s:{index}"),
                &format!("title={}", subtitle.name),
            ],
        );
    }
    if cover_input.is_some() {
        // The cover is the picture stream after the video's own, if it has one.
        let at = usize::from(has_picture && !mp3);
        push(
            &mut arguments,
            &[
                &format!("-c:v:{at}"),
                "mjpeg",
                &format!("-disposition:v:{at}"),
                "attached_pic",
            ],
        );
        if mp3 {
            push(&mut arguments, &["-id3v2_version", "3"]);
        }
    }
    if let (Some(cover), true) = (cover, cover_as_attachment) {
        arguments.push("-attach".into());
        arguments.push(cover.into());
        let name = format!(
            "cover.{}",
            match picture_mime(cover) {
                "image/png" => "png",
                "image/webp" => "webp",
                _ => "jpg",
            }
        );
        push(
            &mut arguments,
            &[
                "-metadata:s:t:0",
                &format!("mimetype={}", picture_mime(cover)),
                "-metadata:s:t:0",
                &format!("filename={name}"),
            ],
        );
    }
    if let Some(title) = plan
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
    {
        push(&mut arguments, &["-metadata", &format!("title={title}")]);
    }
    arguments.push(output.into());
    arguments
}

/// Where a file that can't go inside the video is put: beside it, under a name nothing
/// there already has.
pub fn beside(video: &Path, name: &str) -> PathBuf {
    let folder = video.parent().unwrap_or(Path::new("."));
    let wanted = folder.join(name);
    if !wanted.exists() {
        return wanted;
    }
    let name = Path::new(name);
    let stem = name.file_stem().unwrap_or_default().to_string_lossy();
    let extension = name
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()));
    (2..)
        .map(|count| {
            folder.join(format!(
                "{stem} ({count}){}",
                extension.as_deref().unwrap_or("")
            ))
        })
        .find(|path| !path.exists())
        .unwrap_or(wanted)
}

/// Moves a file, copying it when the two places are on different drives.
pub fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to).or_else(|_| {
        fs::copy(from, to)?;
        fs::remove_file(from)
    })
}

/// Deletes a file, trying again for a moment if something else still has it open.
pub fn remove_patiently(path: &Path, attempts: u32, wait: Duration) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 0..attempts.max(1) {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => last = error.to_string(),
        }
        if attempt + 1 < attempts {
            std::thread::sleep(wait);
        }
    }
    Err(last)
}

/// Puts the plan's subtitles and cover beside `video`, as `Film.en.vtt` and `Film.jpg`,
/// leaving out any that went inside. Returns where each went.
pub fn put_extras_beside(
    video: &Path,
    plan: &FinishPlan,
    inside: bool,
) -> Result<Vec<PathBuf>, String> {
    let container = extension_of(video);
    let room = if inside {
        room_in(&container)
    } else {
        Room {
            subtitles: false,
            cover: false,
        }
    };
    let stem = video
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "video".to_owned());
    let mut placed = Vec::new();
    let mut place = |from: &Path, name: String| -> Result<(), String> {
        if !from.exists() {
            return Ok(());
        }
        let to = beside(video, &name);
        move_file(from, &to).map_err(|error| text!("couldn't save {name}: {error}"))?;
        placed.push(to);
        Ok(())
    };
    if !room.subtitles {
        for subtitle in &plan.subtitles {
            let extension = extension_of(&subtitle.path);
            place(
                &subtitle.path,
                format!("{stem}.{}.{extension}", subtitle.language),
            )?;
        }
    }
    if let Some(cover) = plan.cover.as_deref().filter(|_| !room.cover) {
        place(cover, format!("{stem}.{}", extension_of(cover)))?;
    }
    Ok(placed)
}

/// Makes the finished file: joins `sound` into `video` if it's separate, puts in the plan's
/// subtitles and cover where the file has room, and turns sound into MP3 if asked. The
/// result takes the video's place, or sits beside it for an MP3. Extras without room go
/// beside it. On failure the downloaded files are left as they were.
pub fn finish_files(
    ffmpeg: &Path,
    video: &Path,
    sound: Option<&Path>,
    plan: &FinishPlan,
) -> Result<Finished, String> {
    let output = if plan.mp3 {
        let stem = video.file_stem().unwrap_or_default().to_string_lossy();
        beside(video, &format!("{stem}.mp3"))
    } else {
        video.to_owned()
    };
    let container = extension_of(&output);
    let room = room_in(&container);
    let anything_inside =
        (room.subtitles && !plan.subtitles.is_empty()) || (room.cover && plan.cover.is_some());

    // Nothing for ffmpeg to do: the extras simply go beside the video.
    if sound.is_none() && !plan.mp3 && plan.section.is_none() && !anything_inside {
        put_extras_beside(video, plan, false)?;
        let size = fs::metadata(video)
            .map_err(|error| text!("the video can't be read: {error}"))?
            .len();
        return Ok(Finished {
            path: video.to_owned(),
            size,
            leftover: None,
        });
    }

    // ffmpeg picks the format from the name, so the extension has to stay last.
    let working = output.with_extension(format!("finishing.{container}"));
    let status = Command::new(ffmpeg)
        .args(arguments(video, sound, plan, !plan.audio_only, &working))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|error| text!("ffmpeg couldn't start: {error}"))?;

    // A program that says it worked but made nothing failed all the same.
    if !status.success() || !working.is_file() {
        let _ = fs::remove_file(&working);
        return Err(if sound.is_some() {
            text!("ffmpeg couldn't join the picture and sound, so they stay as two files")
        } else if plan.mp3 {
            text!(
                "ffmpeg couldn't make an MP3, so the sound stays as {kind}",
                kind = extension_of(video).to_ascii_uppercase()
            )
        } else {
            text!("ffmpeg couldn't put the subtitles and cover in the video")
        });
    }

    let size = fs::metadata(&working)
        .map_err(|error| text!("the finished file can't be read: {error}"))?
        .len();
    // ffmpeg wrote a new file, which needs the internet tag the downloaded one had.
    engine::copy_mark(video, &working);
    fs::rename(&working, &output).map_err(|error| text!("could not save the file: {error}"))?;
    if output != video {
        remove_patiently(video, DELETE_ATTEMPTS, DELETE_WAIT).map_err(|error| {
            text!("the MP3 is saved, but the original couldn't be deleted: {error}")
        })?;
    }
    put_extras_beside(&output, plan, true)?;
    let leftover =
        sound.and_then(|sound| remove_patiently(sound, DELETE_ATTEMPTS, DELETE_WAIT).err());
    Ok(Finished {
        path: output,
        size,
        leftover,
    })
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    fn text(arguments: &[OsString]) -> String {
        arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn plan_with(subtitles: &[(&str, &str)], cover: Option<&str>) -> FinishPlan {
        FinishPlan {
            mp3: false,
            audio_only: false,
            subtitles: subtitles
                .iter()
                .map(|(language, file)| PlannedSubtitle {
                    path: PathBuf::from(file),
                    language: (*language).to_owned(),
                    name: format!("{language} name"),
                })
                .collect(),
            cover: cover.map(PathBuf::from),
            title: Some("A film".to_owned()),
            section: None,
        }
    }

    #[test]
    fn a_plan_survives_being_saved() {
        let plan = plan_with(&[("en", r"C:\t\1.en.vtt")], Some(r"C:\t\1.jpg"));
        assert_eq!(FinishPlan::read(&plan.write()), Some(plan));
        assert_eq!(FinishPlan::read("{}"), Some(FinishPlan::default()));
        assert!(FinishPlan::default().is_empty());
        assert_eq!(FinishPlan::read("not json"), None);
    }

    #[test]
    fn picture_and_sound_are_joined_with_nothing_else() {
        let arguments = arguments(
            Path::new("v.mp4"),
            Some(Path::new("a.m4a")),
            &FinishPlan::default(),
            true,
            Path::new("v.finishing.mp4"),
        );
        assert_eq!(
            text(&arguments),
            "-y -loglevel error -i v.mp4 -i a.m4a -map 0:v:0 -map 1:a:0 -c copy v.finishing.mp4"
        );
    }

    #[test]
    fn mp4_takes_subtitles_as_text_and_the_cover_as_a_picture() {
        let plan = plan_with(&[("en", "en.vtt"), ("fr", "fr.srt")], Some("c.webp"));
        let arguments = arguments(
            Path::new("v.mp4"),
            Some(Path::new("a.m4a")),
            &plan,
            true,
            Path::new("out.mp4"),
        );
        assert_eq!(
            text(&arguments),
            "-y -loglevel error -i v.mp4 -i a.m4a -i en.vtt -i fr.srt -i c.webp \
             -map 0:v:0 -map 1:a:0 -map 2:0 -map 3:0 -map 4:0 -c copy \
             -c:s:0 mov_text -metadata:s:s:0 language=en -metadata:s:s:0 title=en name \
             -c:s:1 mov_text -metadata:s:s:1 language=fr -metadata:s:s:1 title=fr name \
             -c:v:1 mjpeg -disposition:v:1 attached_pic -metadata title=A film out.mp4"
        );
    }

    #[test]
    fn matroska_keeps_the_cover_as_an_attachment() {
        let plan = plan_with(&[("en", "en.ass")], Some("c.png"));
        let arguments = text(&arguments(
            Path::new("v.mkv"),
            None,
            &plan,
            true,
            Path::new("out.mkv"),
        ));
        assert!(arguments.contains("-i v.mkv -i en.ass -map 0:v:0 -map 0:a? -map 1:0"));
        assert!(arguments.contains("-c:s:0 ass"));
        assert!(arguments.contains(
            "-attach c.png -metadata:s:t:0 mimetype=image/png -metadata:s:t:0 filename=cover.png"
        ));
        assert!(!arguments.contains("-i c.png"));
    }

    #[test]
    fn webm_takes_subtitles_but_no_cover() {
        let plan = plan_with(&[("en", "en.vtt")], Some("c.jpg"));
        let arguments = text(&arguments(
            Path::new("v.webm"),
            None,
            &plan,
            true,
            Path::new("out.webm"),
        ));
        assert!(arguments.contains("-c:s:0 webvtt"));
        assert!(!arguments.contains("c.jpg"));
    }

    #[test]
    fn mp3_is_encoded_with_its_cover_and_title() {
        let plan = FinishPlan {
            mp3: true,
            ..plan_with(&[("en", "en.vtt")], Some("c.jpg"))
        };
        let arguments = text(&arguments(
            Path::new("song.m4a"),
            None,
            &plan,
            false,
            Path::new("song.mp3"),
        ));
        assert_eq!(
            arguments,
            "-y -loglevel error -i song.m4a -i c.jpg -map 0:a:0 -map 1:0 \
             -c:a libmp3lame -q:a 2 -c:v:0 mjpeg -disposition:v:0 attached_pic \
             -id3v2_version 3 -metadata title=A film song.mp3"
        );
    }

    #[test]
    fn a_section_seeks_every_timed_input_but_not_the_cover() {
        let plan = FinishPlan {
            section: Some(Section {
                from: 2.0,
                to: 4.5,
                exact: false,
            }),
            ..plan_with(&[("en", "en.vtt")], Some("c.jpg"))
        };
        let arguments = text(&arguments(
            Path::new("v.mp4"),
            Some(Path::new("a.m4a")),
            &plan,
            true,
            Path::new("out.mp4"),
        ));
        assert!(arguments.contains(
            "-ss 2.000 -to 4.500 -i v.mp4 -ss 2.000 -to 4.500 -i a.m4a \
             -ss 2.000 -to 4.500 -i en.vtt -i c.jpg"
        ));
        assert!(!arguments.contains("libx264"), "a keyframe cut copies");
        assert_eq!(plan.label(), "Cutting the part chosen…");
    }

    #[test]
    fn an_exact_cut_encodes_the_picture_where_the_file_allows() {
        let exact = FinishPlan {
            section: Some(Section {
                from: 1.0,
                to: 3.0,
                exact: true,
            }),
            ..FinishPlan::default()
        };
        let mp4 = text(&arguments(
            Path::new("v.mp4"),
            None,
            &exact,
            true,
            Path::new("o.mp4"),
        ));
        assert!(mp4.contains("-c copy -c:v:0 libx264 -preset veryfast -crf 18 -c:a aac"));
        let webm = text(&arguments(
            Path::new("v.webm"),
            None,
            &exact,
            true,
            Path::new("o.webm"),
        ));
        assert!(!webm.contains("libx264"));
        // Sound alone cuts precisely enough without encoding again.
        let sound = text(&arguments(
            Path::new("s.m4a"),
            None,
            &exact,
            false,
            Path::new("o.m4a"),
        ));
        assert!(!sound.contains("libx264"));
    }

    #[test]
    fn a_section_must_make_sense() {
        let section = |from, to| Section {
            from,
            to,
            exact: false,
        };
        assert!(section(0.0, 5.0).fits(10.0));
        assert!(section(3.0, 50.0).fits(0.0), "of unknown length");
        assert!(!section(5.0, 5.0).fits(10.0));
        assert!(!section(-1.0, 5.0).fits(10.0));
        assert!(!section(12.0, 15.0).fits(10.0));
        assert!(!section(f64::NAN, 5.0).fits(10.0));
    }

    #[test]
    fn extras_go_beside_a_file_with_no_room_for_them() {
        let folder = tempfile::tempdir().unwrap();
        let video = folder.path().join("Film.ts");
        let subtitle = folder.path().join("7.en.vtt");
        let cover = folder.path().join("7.cover.webp");
        fs::write(&video, b"video").unwrap();
        fs::write(&subtitle, b"WEBVTT").unwrap();
        fs::write(&cover, b"picture").unwrap();
        // Something already has the cover's name, so it gets another.
        fs::write(folder.path().join("Film.webp"), b"other").unwrap();
        let plan = FinishPlan {
            subtitles: vec![PlannedSubtitle {
                path: subtitle.clone(),
                language: "en".to_owned(),
                name: "English".to_owned(),
            }],
            cover: Some(cover.clone()),
            ..FinishPlan::default()
        };

        // A file ffmpeg isn't needed for: nothing fits in a .ts file.
        let failing = PathBuf::from(env::var_os("ComSpec").unwrap());
        let finished = finish_files(&failing, &video, None, &plan).unwrap();

        assert_eq!(finished.path, video);
        assert_eq!(finished.size, 5);
        assert_eq!(
            fs::read(folder.path().join("Film.en.vtt")).unwrap(),
            b"WEBVTT"
        );
        assert_eq!(
            fs::read(folder.path().join("Film (2).webp")).unwrap(),
            b"picture"
        );
        assert!(!subtitle.exists() && !cover.exists());
    }

    #[test]
    fn a_failed_join_keeps_both_files() {
        let folder = tempfile::tempdir().unwrap();
        let video = folder.path().join("a.mp4");
        let audio = folder.path().join("a.audio.m4a");
        fs::write(&video, b"picture").unwrap();
        fs::write(&audio, b"sound").unwrap();

        // Any program that fails stands in for ffmpeg giving up.
        let failing = PathBuf::from(env::var_os("ComSpec").unwrap());
        let problem =
            finish_files(&failing, &video, Some(&audio), &FinishPlan::default()).unwrap_err();

        assert!(problem.contains("stay as two files"), "{problem}");
        assert_eq!(fs::read(&video).unwrap(), b"picture");
        assert_eq!(fs::read(&audio).unwrap(), b"sound");
        assert_eq!(
            fs::read_dir(folder.path()).unwrap().count(),
            2,
            "nothing half-made is left"
        );
    }

    #[test]
    fn a_failed_mp3_keeps_the_sound() {
        let folder = tempfile::tempdir().unwrap();
        let sound = folder.path().join("song.m4a");
        fs::write(&sound, b"sound").unwrap();
        let plan = FinishPlan {
            mp3: true,
            audio_only: true,
            ..FinishPlan::default()
        };

        let failing = PathBuf::from(env::var_os("ComSpec").unwrap());
        let problem = finish_files(&failing, &sound, None, &plan).unwrap_err();

        assert!(problem.contains("stays as M4A"), "{problem}");
        assert_eq!(fs::read(&sound).unwrap(), b"sound");
    }

    #[test]
    fn a_missing_ffmpeg_is_explained() {
        let folder = tempfile::tempdir().unwrap();
        let video = folder.path().join("a.mp4");
        fs::write(&video, b"picture").unwrap();

        let problem = finish_files(
            Path::new(r"C:\no\such\ffmpeg.exe"),
            &video,
            Some(&video),
            &FinishPlan::default(),
        )
        .unwrap_err();
        assert!(problem.contains("ffmpeg couldn't start"), "{problem}");
    }
}
