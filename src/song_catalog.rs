//! 10 reference songs for lyrics accuracy + timing QA.

#[derive(Debug, Clone)]
pub struct SongRef {
    pub id: &'static str,
    pub artist: &'static str,
    pub track: &'static str,
    pub duration_s: f64,
    pub must_contain: &'static [&'static str],
    pub must_not_contain: &'static [&'static str],
    pub youtube_titles: &'static [&'static str],
    pub min_timed_lines: usize,
    pub min_coverage: f64,
}

pub const SONG_CATALOG: &[SongRef] = &[
    SongRef {
        id: "cas_apocalypse",
        artist: "Cigarettes After Sex",
        track: "Apocalypse",
        duration_s: 290.0,
        must_contain: &["leapt", "crumbling", "cityscapes"],
        must_not_contain: &["firefighter", "falling angel", "burning house"],
        youtube_titles: &[
            "Apocalypse - Cigarettes After Sex",
            "Cigarettes After Sex - Apocalypse",
            "Apocalypse - Cigarettes After Sex (Official Audio)",
        ],
        min_timed_lines: 12,
        min_coverage: 0.50,
    },
    SongRef {
        id: "weeknd_blinding",
        artist: "The Weeknd",
        track: "Blinding Lights",
        duration_s: 200.0,
        must_contain: &["tryna call", "sin city"],
        must_not_contain: &["starboy", "the hills"],
        youtube_titles: &[
            "The Weeknd - Blinding Lights (Official Video)",
            "Blinding Lights - The Weeknd",
        ],
        min_timed_lines: 15,
        min_coverage: 0.55,
    },
    SongRef {
        id: "billie_bad_guy",
        artist: "Billie Eilish",
        track: "bad guy",
        duration_s: 194.0,
        must_contain: &["white shirt", "duh"],
        must_not_contain: &["ocean eyes"],
        youtube_titles: &["Billie Eilish - bad guy", "bad guy - Billie Eilish"],
        min_timed_lines: 12,
        min_coverage: 0.50,
    },
    SongRef {
        id: "ed_shape",
        artist: "Ed Sheeran",
        track: "Shape of You",
        duration_s: 233.0,
        must_contain: &["club isn't the best place", "shape of you"],
        must_not_contain: &["thinking out loud"],
        youtube_titles: &[
            "Ed Sheeran - Shape of You (Official Music Video)",
            "Shape of You - Ed Sheeran",
        ],
        min_timed_lines: 15,
        min_coverage: 0.55,
    },
    SongRef {
        id: "adele_hello",
        artist: "Adele",
        track: "Hello",
        duration_s: 295.0,
        must_contain: &["hello from the other side", "hello"],
        must_not_contain: &["rolling in the deep"],
        youtube_titles: &["Adele - Hello", "Hello - Adele"],
        min_timed_lines: 10,
        min_coverage: 0.45,
    },
    SongRef {
        id: "queen_bohemian",
        artist: "Queen",
        track: "Bohemian Rhapsody",
        duration_s: 355.0,
        must_contain: &["is this the real life", "scaramouch"],
        must_not_contain: &["we will rock you"],
        youtube_titles: &["Queen - Bohemian Rhapsody", "Bohemian Rhapsody - Queen"],
        min_timed_lines: 20,
        min_coverage: 0.55,
    },
    SongRef {
        id: "rick_nggyu",
        artist: "Rick Astley",
        track: "Never Gonna Give You Up",
        duration_s: 213.0,
        must_contain: &["never gonna give you up", "never gonna let you down"],
        must_not_contain: &["together forever"],
        youtube_titles: &[
            "Rick Astley - Never Gonna Give You Up (Official Video)",
            "Never Gonna Give You Up - Rick Astley",
        ],
        min_timed_lines: 12,
        min_coverage: 0.50,
    },
    SongRef {
        id: "imagine_believer",
        artist: "Imagine Dragons",
        track: "Believer",
        duration_s: 204.0,
        must_contain: &["first things first", "believer"],
        must_not_contain: &["radioactive"],
        youtube_titles: &["Imagine Dragons - Believer", "Believer - Imagine Dragons"],
        min_timed_lines: 12,
        min_coverage: 0.50,
    },
    SongRef {
        id: "dua_levitating",
        artist: "Dua Lipa",
        track: "Levitating",
        duration_s: 203.0,
        must_contain: &["run away with me", "levitating"],
        must_not_contain: &["don't start now"],
        youtube_titles: &["Dua Lipa - Levitating", "Levitating - Dua Lipa"],
        min_timed_lines: 12,
        min_coverage: 0.50,
    },
    SongRef {
        id: "coldplay_viva",
        artist: "Coldplay",
        track: "Viva La Vida",
        duration_s: 242.0,
        must_contain: &["i used to rule the world", "castles"],
        must_not_contain: &["yellow", "the scientist"],
        youtube_titles: &["Coldplay - Viva La Vida", "Viva La Vida - Coldplay"],
        min_timed_lines: 12,
        min_coverage: 0.50,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_ten() {
        assert_eq!(SONG_CATALOG.len(), 10);
    }
}
