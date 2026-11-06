//! DASH manifests.

use dash_mpd::{AdaptationSet, MPD, Period, Representation, S, SegmentList, SegmentTemplate};
use url::Url;

use crate::{
    ByteRange, Error, MAX_PARTS, Part, Quality, Track, audio_label, joined_extension, parse_base,
    resolve, too_many_parts, video_label,
};

/// What marks a manifest as protected. `ContentProtection` is the standard way, and the
/// others are how a key or licence can appear beside it.
const PROTECTION_MARKS: [&str; 4] = ["contentprotection", "default_kid", "pssh", "mspr:pro"];

fn check_protection(xml: &str) -> Result<(), Error> {
    let lower = xml.to_ascii_lowercase();
    if PROTECTION_MARKS.iter().any(|mark| lower.contains(mark)) {
        Err(Error::Protected)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Video,
    Audio,
}

/// One representation of one period.
struct Rendition {
    kind: Kind,
    width: Option<u64>,
    height: Option<u64>,
    bandwidth: Option<u64>,
    track: Track,
}

/// Fills in a template such as `seg-$Number%05d$.m4s`.
fn expand(template: &str, id: &str, bandwidth: u64, number: u64, time: u64) -> String {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find('$') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('$') else {
            rest = &rest[start..];
            break;
        };
        let token = &after[..end];
        rest = &after[end + 1..];

        let (name, format) = token.split_once('%').unwrap_or((token, ""));
        let width = format
            .strip_prefix('0')
            .and_then(|digits| digits.strip_suffix('d'))
            .and_then(|digits| digits.parse().ok())
            .unwrap_or(0);

        match name {
            "" => output.push('$'),
            "RepresentationID" => output.push_str(id),
            "Number" => output.push_str(&format!("{number:0width$}")),
            "Time" => output.push_str(&format!("{time:0width$}")),
            "Bandwidth" => output.push_str(&format!("{bandwidth:0width$}")),
            _ => {
                output.push('$');
                output.push_str(token);
                output.push('$');
            }
        }
    }

    output.push_str(rest);
    output
}

/// One entry of a timeline, as its number and start time.
struct Slot {
    number: u64,
    time: u64,
}

/// Lists a timeline's parts. A negative repeat means up to the next entry, or the end.
fn timeline_slots(entries: &[S], first_number: u64, end_time: u64) -> Result<Vec<Slot>, Error> {
    let mut slots = Vec::new();
    let mut time = 0;
    let mut number = first_number;

    for (index, entry) in entries.iter().enumerate() {
        if let Some(start) = entry.t {
            time = start;
        }
        if entry.d == 0 {
            return Err(Error::Invalid("a timeline has an empty entry".to_owned()));
        }

        let count = match entry.r.unwrap_or(0) {
            repeat if repeat >= 0 => repeat as u64 + 1,
            _ => {
                let stop = entries
                    .get(index + 1)
                    .and_then(|next| next.t)
                    .unwrap_or(end_time);
                stop.saturating_sub(time).div_ceil(entry.d)
            }
        };
        if count > MAX_PARTS as u64 || slots.len() + count as usize > MAX_PARTS {
            return Err(too_many_parts());
        }

        for _ in 0..count {
            slots.push(Slot { number, time });
            number += 1;
            time += entry.d;
        }
    }
    Ok(slots)
}

/// Lists a template's parts when it gives one fixed length instead of a timeline.
fn fixed_slots(
    first_number: u64,
    part_length: f64,
    timescale: u64,
    seconds: f64,
) -> Result<Vec<Slot>, Error> {
    if part_length <= 0.0 {
        return Err(Error::Invalid("a template has no part length".to_owned()));
    }

    let count = (seconds * timescale as f64 / part_length).ceil() as u64;
    if count > MAX_PARTS as u64 {
        return Err(too_many_parts());
    }
    Ok((0..count)
        .map(|index| Slot {
            number: first_number + index,
            time: (index as f64 * part_length) as u64,
        })
        .collect())
}

/// A template, with anything it leaves out taken from the levels above it.
fn merged_template(
    period: &Period,
    adaptation: &AdaptationSet,
    representation: &Representation,
) -> Option<SegmentTemplate> {
    let levels = [
        representation.SegmentTemplate.as_ref(),
        adaptation.SegmentTemplate.as_ref(),
        period.SegmentTemplate.as_ref(),
    ];
    let mut levels = levels.into_iter().flatten();
    let mut merged = levels.next()?.clone();

    for level in levels {
        merged.media = merged.media.or_else(|| level.media.clone());
        merged.initialization = merged
            .initialization
            .or_else(|| level.initialization.clone());
        merged.Initialization = merged
            .Initialization
            .or_else(|| level.Initialization.clone());
        merged.startNumber = merged.startNumber.or(level.startNumber);
        merged.duration = merged.duration.or(level.duration);
        merged.timescale = merged.timescale.or(level.timescale);
        merged.SegmentTimeline = merged
            .SegmentTimeline
            .or_else(|| level.SegmentTimeline.clone());
    }
    Some(merged)
}

fn template_parts(
    base: &Url,
    template: &SegmentTemplate,
    id: &str,
    bandwidth: u64,
    seconds: f64,
) -> Result<Vec<Part>, Error> {
    let media = template
        .media
        .as_deref()
        .ok_or_else(|| Error::Invalid("a template has no address".to_owned()))?;
    let timescale = template.timescale.unwrap_or(1).max(1);
    let first_number = template.startNumber.unwrap_or(1);

    let slots = match &template.SegmentTimeline {
        Some(timeline) => {
            let end_time = (seconds * timescale as f64) as u64;
            timeline_slots(&timeline.segments, first_number, end_time)?
        }
        None => fixed_slots(
            first_number,
            template.duration.unwrap_or(0.0),
            timescale,
            seconds,
        )?,
    };

    let mut parts = Vec::with_capacity(slots.len() + 1);
    let initialization = template.initialization.as_deref().or(template
        .Initialization
        .as_ref()
        .and_then(|item| item.sourceURL.as_deref()));
    if let Some(initialization) = initialization {
        let link = expand(initialization, id, bandwidth, first_number, 0);
        parts.push(Part {
            url: resolve(base, &link)?.into(),
            range: None,
        });
    }

    for slot in slots {
        let link = expand(media, id, bandwidth, slot.number, slot.time);
        parts.push(Part {
            url: resolve(base, &link)?.into(),
            range: None,
        });
    }
    Ok(parts)
}

/// Reads `first-last`, as DASH writes byte ranges.
fn parse_range(text: &str) -> Option<ByteRange> {
    let (first, last) = text.split_once('-')?;
    let start: u64 = first.trim().parse().ok()?;
    let last: u64 = last.trim().parse().ok()?;
    Some(ByteRange {
        start,
        length: last.checked_sub(start)? + 1,
    })
}

fn list_parts(base: &Url, list: &SegmentList) -> Result<Vec<Part>, Error> {
    if list.segment_urls.len() > MAX_PARTS {
        return Err(too_many_parts());
    }

    let mut parts = Vec::with_capacity(list.segment_urls.len() + 1);
    if let Some(initialization) = &list.Initialization {
        parts.push(Part {
            url: match &initialization.sourceURL {
                Some(link) => resolve(base, link)?.into(),
                None => base.to_string(),
            },
            range: initialization.range.as_deref().and_then(parse_range),
        });
    }

    for item in &list.segment_urls {
        parts.push(Part {
            url: match &item.media {
                Some(link) => resolve(base, link)?.into(),
                None => base.to_string(),
            },
            range: item.mediaRange.as_deref().and_then(parse_range),
        });
    }
    Ok(parts)
}

fn kind_of(adaptation: &AdaptationSet, representation: &Representation) -> Option<Kind> {
    let declared = representation
        .contentType
        .as_deref()
        .or(adaptation.contentType.as_deref())
        .or(representation.mimeType.as_deref())
        .or(adaptation.mimeType.as_deref())
        .unwrap_or_default();

    if declared.starts_with("video") {
        Some(Kind::Video)
    } else if declared.starts_with("audio") {
        Some(Kind::Audio)
    } else if representation.width.is_some() || adaptation.width.is_some() {
        Some(Kind::Video)
    } else {
        None
    }
}

fn join_base(base: &Url, levels: &[Option<&String>]) -> Result<Url, Error> {
    let mut joined = base.clone();
    for level in levels.iter().flatten() {
        joined = resolve(&joined, level)?;
    }
    Ok(joined)
}

fn extension(
    representation: &Representation,
    adaptation: &AdaptationSet,
    parts: &[Part],
    kind: Kind,
) -> String {
    let audio = kind == Kind::Audio;
    let mime = representation
        .mimeType
        .as_deref()
        .or(adaptation.mimeType.as_deref())
        .unwrap_or_default();

    if mime.ends_with("/webm") {
        "webm".to_owned()
    } else if mime.ends_with("/mp4") {
        if audio { "m4a" } else { "mp4" }.to_owned()
    } else {
        let url = parts.last().map_or("", |part| part.url.as_str());
        joined_extension(url, true, audio)
    }
}

fn period_seconds(mpd: &MPD, index: usize) -> Option<f64> {
    let period = &mpd.periods[index];
    if let Some(duration) = period.duration {
        return Some(duration.as_secs_f64());
    }

    let start = period.start.map_or(0.0, |start| start.as_secs_f64());
    let end = mpd
        .periods
        .get(index + 1)
        .and_then(|next| next.start)
        .or(mpd.mediaPresentationDuration)?;
    Some(end.as_secs_f64() - start)
}

fn rendition(
    base: &Url,
    period: &Period,
    adaptation: &AdaptationSet,
    representation: &Representation,
    seconds: f64,
) -> Result<Option<Rendition>, Error> {
    let Some(kind) = kind_of(adaptation, representation) else {
        return Ok(None);
    };

    let base = join_base(
        base,
        &[
            adaptation.BaseURL.first().map(|item| &item.base),
            representation.BaseURL.first().map(|item| &item.base),
        ],
    )?;
    let id = representation.id.as_deref().unwrap_or_default();
    let bandwidth = representation.bandwidth;

    let list = representation
        .SegmentList
        .as_ref()
        .or(adaptation.SegmentList.as_ref())
        .or(period.SegmentList.as_ref());

    let parts = if let Some(template) = merged_template(period, adaptation, representation) {
        template_parts(&base, &template, id, bandwidth.unwrap_or(0), seconds)?
    } else if let Some(list) = list {
        list_parts(&base, list)?
    } else {
        // With no parts described, the address is the whole file.
        vec![Part {
            url: base.into(),
            range: None,
        }]
    };

    Ok(Some(Rendition {
        kind,
        width: representation.width.or(adaptation.width),
        height: representation.height.or(adaptation.height),
        bandwidth,
        track: Track {
            extension: extension(representation, adaptation, &parts, kind),
            parts,
            seconds,
        },
    }))
}

/// The renditions of one period, best first, so that later periods can be lined up.
fn period_renditions(mpd: &MPD, base: &Url, index: usize) -> Result<Vec<Rendition>, Error> {
    let period = &mpd.periods[index];
    let seconds = period_seconds(mpd, index)
        .ok_or_else(|| Error::Invalid("it doesn't say how long it is".to_owned()))?;
    let base = join_base(base, &[period.BaseURL.first().map(|item| &item.base)])?;

    let mut renditions = Vec::new();
    for adaptation in &period.adaptations {
        for representation in &adaptation.representations {
            if let Some(found) = rendition(&base, period, adaptation, representation, seconds)? {
                renditions.push(found);
            }
        }
    }
    renditions.sort_by_key(|item| {
        std::cmp::Reverse((item.height.unwrap_or(0), item.bandwidth.unwrap_or(0)))
    });
    Ok(renditions)
}

/// Adds a later period's parts to the same position in the list of qualities.
fn append_period(all: &mut [Rendition], later: Vec<Rendition>) {
    for kind in [Kind::Video, Kind::Audio] {
        let matching: Vec<&Rendition> = later.iter().filter(|item| item.kind == kind).collect();
        let Some(last) = matching.last() else {
            continue;
        };

        let targets = all.iter_mut().filter(|item| item.kind == kind);
        for (position, target) in targets.enumerate() {
            let source = matching.get(position).unwrap_or(last);
            target
                .track
                .parts
                .extend(source.track.parts.iter().cloned());
            target.track.seconds += source.track.seconds;
        }
    }
}

pub(crate) fn read(url: &str, xml: &str) -> Result<Vec<Quality>, Error> {
    check_protection(xml)?;

    let mpd = dash_mpd::parse(xml).map_err(|error| Error::Invalid(error.to_string()))?;
    if mpd.mpdtype.as_deref() == Some("dynamic") {
        return Err(Error::Live);
    }
    if mpd.periods.is_empty() {
        return Err(Error::Invalid("it lists nothing to save".to_owned()));
    }

    let base = parse_base(url)?;
    let base = join_base(&base, &[mpd.base_url.first().map(|item| &item.base)])?;

    let mut renditions = period_renditions(&mpd, &base, 0)?;
    for index in 1..mpd.periods.len() {
        append_period(&mut renditions, period_renditions(&mpd, &base, index)?);
    }

    // The first audio listed is the one the site means by default, so its best version
    // goes with every picture quality.
    let audio = renditions
        .iter()
        .filter(|item| item.kind == Kind::Audio)
        .max_by_key(|item| item.bandwidth.unwrap_or(0))
        .map(|item| item.track.clone());

    let videos: Vec<&Rendition> = renditions
        .iter()
        .filter(|item| item.kind == Kind::Video)
        .collect();

    if videos.is_empty() {
        return Ok(renditions
            .iter()
            .map(|item| Quality {
                label: audio_label(item.bandwidth),
                width: None,
                height: None,
                bandwidth: item.bandwidth,
                video: item.track.clone(),
                audio: None,
            })
            .collect());
    }

    Ok(videos
        .into_iter()
        .map(|item| Quality {
            label: video_label(item.height, item.bandwidth),
            width: item.width,
            height: item.height,
            bandwidth: item.bandwidth,
            video: item.track.clone(),
            audio: audio.clone(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_fill_in_every_kind_of_value() {
        let fill = |template| expand(template, "v1", 2000, 7, 3600);

        assert_eq!(fill("$RepresentationID$/a.m4s"), "v1/a.m4s");
        assert_eq!(fill("seg-$Number$.m4s"), "seg-7.m4s");
        assert_eq!(fill("seg-$Number%05d$.m4s"), "seg-00007.m4s");
        assert_eq!(fill("$Time$.m4s"), "3600.m4s");
        assert_eq!(fill("$Time%06d$.m4s"), "003600.m4s");
        assert_eq!(fill("$Bandwidth$.m4s"), "2000.m4s");
        assert_eq!(fill("cost$$.m4s"), "cost$.m4s");
        assert_eq!(fill("$Unknown$.m4s"), "$Unknown$.m4s");
        assert_eq!(fill("end$"), "end$");
    }

    fn entry(t: Option<u64>, d: u64, r: Option<i64>) -> S {
        S {
            t,
            d,
            r,
            ..S::default()
        }
    }

    fn times(slots: &[Slot]) -> Vec<u64> {
        slots.iter().map(|slot| slot.time).collect()
    }

    #[test]
    fn a_timeline_repeats_and_keeps_counting() {
        let entries = [entry(Some(100), 10, Some(2)), entry(None, 5, None)];
        let slots = timeline_slots(&entries, 4, 0).unwrap();

        assert_eq!(times(&slots), [100, 110, 120, 130]);
        assert_eq!(slots.last().unwrap().number, 7);
    }

    #[test]
    fn a_negative_repeat_runs_to_the_next_entry_or_the_end() {
        let entries = [entry(Some(0), 10, Some(-1)), entry(Some(30), 5, Some(-1))];
        let slots = timeline_slots(&entries, 1, 40).unwrap();

        assert_eq!(times(&slots), [0, 10, 20, 30, 35]);
    }

    #[test]
    fn a_timeline_cannot_describe_an_endless_list() {
        let entries = [entry(Some(0), 1, Some(i64::MAX))];
        assert!(timeline_slots(&entries, 1, 0).is_err());

        let entries = [entry(Some(0), 0, None)];
        assert!(timeline_slots(&entries, 1, 0).is_err());
    }

    #[test]
    fn a_fixed_length_covers_the_whole_duration() {
        let slots = fixed_slots(1, 4.0, 1, 10.0).unwrap();
        assert_eq!(times(&slots), [0, 4, 8]);

        assert!(fixed_slots(1, 0.0, 1, 10.0).is_err());
        assert!(fixed_slots(1, 1.0, 1, 1e12).is_err());
    }

    #[test]
    fn ranges_include_both_ends() {
        let range = parse_range("100-199").unwrap();
        assert_eq!((range.start, range.length), (100, 100));

        assert_eq!(parse_range("nonsense"), None);
        assert_eq!(parse_range("9-3"), None);
    }

    #[test]
    fn protection_is_found_however_it_is_written() {
        for xml in [
            "<ContentProtection schemeIdUri=\"x\"/>",
            "<mpd:ContentProtection/>",
            "<contentprotection/>",
            "<AdaptationSet cenc:default_KID=\"1\"/>",
            "<cenc:pssh>AAAA</cenc:pssh>",
            "<mspr:pro>AAAA</mspr:pro>",
        ] {
            assert_eq!(check_protection(xml), Err(Error::Protected), "{xml}");
        }
        assert_eq!(check_protection("<MPD></MPD>"), Ok(()));
    }

    #[test]
    fn a_template_inherits_what_it_leaves_out() {
        let xml = r#"<MPD type="static" mediaPresentationDuration="PT8S"><Period>
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <SegmentTemplate timescale="2" duration="8" initialization="init.mp4"/>
              <Representation id="a" bandwidth="1" width="10" height="10">
                <SegmentTemplate media="$Number$.m4s"/>
              </Representation>
            </AdaptationSet></Period></MPD>"#;
        let qualities = read("https://x.test/m.mpd", xml).unwrap();

        let urls: Vec<&str> = qualities[0]
            .video
            .parts
            .iter()
            .map(|part| part.url.as_str())
            .collect();
        assert_eq!(
            urls,
            [
                "https://x.test/init.mp4",
                "https://x.test/1.m4s",
                "https://x.test/2.m4s"
            ]
        );
    }

    #[test]
    fn periods_join_up_in_order() {
        let xml = r#"<MPD type="static"><Period duration="PT4S">
            <AdaptationSet contentType="video"><SegmentTemplate duration="4" media="one-$Number$.m4s"/>
              <Representation id="a" bandwidth="1" width="10" height="10"/></AdaptationSet>
            </Period><Period duration="PT4S">
            <AdaptationSet contentType="video"><SegmentTemplate duration="4" media="two-$Number$.m4s"/>
              <Representation id="a" bandwidth="1" width="10" height="10"/></AdaptationSet>
            </Period></MPD>"#;
        let qualities = read("https://x.test/m.mpd", xml).unwrap();

        let video = &qualities[0].video;
        assert_eq!(video.parts.len(), 2);
        assert!(video.parts[1].url.ends_with("two-1.m4s"));
        assert_eq!(video.seconds, 8.0);
    }

    #[test]
    fn base_addresses_stack_up() {
        let xml = r#"<MPD type="static" mediaPresentationDuration="PT4S">
            <BaseURL>https://cdn.test/a/</BaseURL><Period><BaseURL>b/</BaseURL>
            <AdaptationSet contentType="video"><BaseURL>c/</BaseURL>
              <Representation id="a" bandwidth="1" width="10" height="10"><BaseURL>d.mp4</BaseURL>
              </Representation></AdaptationSet></Period></MPD>"#;
        let qualities = read("https://x.test/m.mpd", xml).unwrap();

        assert!(qualities[0].video.is_single_file());
        assert_eq!(
            qualities[0].video.parts[0].url,
            "https://cdn.test/a/b/c/d.mp4"
        );
    }

    #[test]
    fn sound_alone_is_offered_when_there_is_no_picture() {
        let xml = r#"<MPD type="static" mediaPresentationDuration="PT4S"><Period>
            <AdaptationSet contentType="audio" mimeType="audio/mp4">
              <SegmentTemplate duration="4" media="$Number$.m4s"/>
              <Representation id="a" bandwidth="128000"/></AdaptationSet></Period></MPD>"#;
        let qualities = read("https://x.test/m.mpd", xml).unwrap();

        assert_eq!(qualities[0].label, "Audio 128 kbps");
        assert_eq!(qualities[0].video.extension, "m4a");
        assert_eq!(qualities[0].audio, None);
    }

    #[test]
    fn text_tracks_are_left_out() {
        let xml = r#"<MPD type="static" mediaPresentationDuration="PT4S"><Period>
            <AdaptationSet contentType="text" mimeType="text/vtt">
              <Representation id="s" bandwidth="1"><BaseURL>s.vtt</BaseURL></Representation>
            </AdaptationSet>
            <AdaptationSet contentType="video"><SegmentTemplate duration="4" media="$Number$.m4s"/>
              <Representation id="v" bandwidth="1" width="10" height="10"/></AdaptationSet>
            </Period></MPD>"#;

        assert_eq!(read("https://x.test/m.mpd", xml).unwrap().len(), 1);
    }
}
