//! Original indexed music catalog and non-repeating shuffle selection.
//! Browser streaming replaces disc DMA; menus/save filtering remain callers.
use crate::{Result, engine_audio::OriginalRandom, physics_data::PhysicsData};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Song {
    pub filename: String,
    pub title: String,
    pub artist: String,
    pub genre: String,
    pub locked: bool,
}
#[derive(Debug, Serialize)]
pub struct MusicLibrary {
    pub songs: Vec<Song>,
}
impl MusicLibrary {
    /// Ordered=1 describes indexed artist/genre references in002f8ab0,
    /// not sequential playback. Other catalog schemas are rejected explicitly.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let data = PhysicsData::parse(bytes)?;
        let integer = |section: &str, key: &str| -> Result<usize> {
            data.first(section, key)?
                .value
                .parse()
                .map_err(|_| format!("invalid [{section}] {key}"))
        };
        if integer("Songs", "Ordered")? != 1 {
            return Err("unsupported unordered music catalog".into());
        }
        let count = integer("Songs", "NumberOfSongs")?;
        if count == 0 || count > 256 {
            return Err("invalid song count".into());
        }
        let text = |section: &str, key: &str| -> Result<String> {
            Ok(data.first(section, key)?.value.trim_matches('"').to_owned())
        };
        let artist_count = integer("Artists", "NumberOfArtists")?;
        let genre_count = integer("Genres", "NumberOfGenres")?;
        let mut songs = Vec::with_capacity(count);
        for i in 1..=count {
            let section = format!("Song {i}");
            let artist = integer(&section, "Artist")?;
            let genre = integer(&section, "Genre")?;
            if artist == 0 || artist > artist_count || genre == 0 || genre > genre_count {
                return Err("song references missing artist/genre".into());
            }
            songs.push(Song {
                filename: text(&section, "Filename")?,
                title: text(&section, "Title")?,
                artist: text("Artists", &format!("Artist{artist}"))?,
                genre: text("Genres", &format!("Genre{genre}"))?,
                locked: integer(&section, "Locked")? & 1 != 0,
            });
        }
        Ok(Self { songs })
    }
}

/// Selection kernel0014ff00: random starting slot, then linear search past
/// current and already-played songs. A full cycle clears played history.
pub struct MusicShuffle {
    available: Vec<usize>,
    played: Vec<usize>,
    current: usize,
}
impl MusicShuffle {
    pub fn new(library: &MusicLibrary) -> Self {
        Self {
            available: library
                .songs
                .iter()
                .enumerate()
                .filter(|(_, s)| !s.locked)
                .map(|(i, _)| i)
                .collect(),
            played: Vec::new(),
            current: 0,
        }
    }
    pub fn next_song(&mut self, random: &mut OriginalRandom) -> Result<Option<usize>> {
        let count = self.available.len();
        if count == 0 {
            return Ok(None);
        }
        let mut slot =
            ((count as f32 * random.next_u31() as f32) * f32::from_bits(0x30000000)) as usize;
        if slot >= count {
            return Err("music random slot outside playlist".into());
        }
        if count > 1 {
            if self.played.len() == count {
                self.played.clear();
            }
            for _ in 0..count {
                let song = self.available[slot];
                if song != self.current && !self.played.contains(&song) {
                    break;
                }
                slot = (slot + 1) % count;
            }
        }
        let song = self.available[slot];
        if !self.played.contains(&song) {
            self.played.push(song);
        }
        self.current = song;
        Ok(Some(song))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shuffle_visits_every_available_song_before_repeating() {
        let library = MusicLibrary {
            songs: (0..5)
                .map(|i| Song {
                    filename: i.to_string(),
                    title: String::new(),
                    artist: String::new(),
                    genre: String::new(),
                    locked: i == 4,
                })
                .collect(),
        };
        let mut shuffle = MusicShuffle::new(&library);
        let mut rng = OriginalRandom::with_seed(1);
        let mut last = None;
        for _ in 0..8 {
            let mut cycle = Vec::new();
            for _ in 0..4 {
                let song = shuffle.next_song(&mut rng).unwrap().unwrap();
                assert_ne!(Some(song), last);
                assert!(!cycle.contains(&song));
                cycle.push(song);
                last = Some(song);
            }
            cycle.sort();
            assert_eq!(cycle, [0, 1, 2, 3]);
        }
    }
}
