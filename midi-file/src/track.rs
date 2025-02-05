use midly::{MidiMessage, TrackEvent, TrackEventKind};
use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::tempo_track::TempoTrack;

#[derive(Debug, Clone)]
pub struct MidiEvent {
    pub channel: u8,
    pub timestamp: Duration,
    pub message: MidiMessage,
    pub track_id: usize,
    pub track_color_id: usize,
}

#[derive(Debug, Clone)]
pub struct ProgramEvent {
    pub channel: u8,
    pub timestamp: Duration,
    pub program: u8,
}

#[derive(Debug, Clone)]
pub struct MidiNote {
    pub start: Duration,
    pub end: Duration,
    pub duration: Duration,
    pub note: u8,
    pub velocity: u8,
    pub channel: u8,
    pub track_id: usize,
    pub track_color_id: usize,
}

#[derive(Debug, Clone)]
pub struct MidiTrack {
    // Translated notes with calculated timings
    pub notes: Arc<[MidiNote]>,

    pub events: Arc<[MidiEvent]>,

    pub track_id: usize,
    pub track_color_id: usize,

    pub programs: Arc<[ProgramEvent]>,
    pub has_drums: bool,
    pub has_other_than_drums: bool,
}

impl MidiTrack {
    pub fn new(
        track_id: usize,
        track_color_id: usize,
        tempo_track: &TempoTrack,
        track_events: &[TrackEvent],
    ) -> Self {
        std::thread::scope(|tb| {
            let notes =
                tb.spawn(|| build_notes(track_id, track_color_id, tempo_track, track_events));

            let events =
                tb.spawn(|| build_events(track_id, track_color_id, tempo_track, track_events));

            let notes = notes.join().unwrap();
            let (events, programs, has_drums, has_other_than_drums) = events.join().unwrap();

            Self {
                track_id,
                track_color_id,
                notes: notes.into(),
                events: events.into(),
                programs: programs.into(),
                has_drums,
                has_other_than_drums,
            }
        })
    }
}

fn build_notes(
    track_id: usize,
    track_color_id: usize,
    tempo_track: &TempoTrack,
    track_events: &[TrackEvent],
) -> Vec<MidiNote> {
    struct NoteInfo {
        velocity: u8,
        channel: u8,
        pulses: u64,
    }

    let mut active_notes: HashMap<u8, NoteInfo> = HashMap::new();
    let mut notes = Vec::new();

    let mut pulses: u64 = 0;
    for event in track_events.iter() {
        pulses += event.delta.as_int() as u64;

        if let TrackEventKind::Midi { channel, message } = event.kind {
            let (key, velocity) = match message {
                MidiMessage::NoteOn { vel, key } => (key.as_int(), vel.as_int()),
                MidiMessage::NoteOff { vel, key } => (key.as_int(), vel.as_int()),
                _ => {
                    continue;
                }
            };

            if let Some(active) = active_notes.remove(&key) {
                let start = active.pulses;
                let end = pulses;

                let start = tempo_track.pulses_to_duration(start);
                let end = tempo_track.pulses_to_duration(end);
                let duration = end - start;

                let note = MidiNote {
                    start,
                    end,
                    duration,
                    note: key,
                    velocity: active.velocity,
                    channel: active.channel,
                    track_id,
                    track_color_id,
                };

                notes.push(note);
            }

            let on = matches!(&message, MidiMessage::NoteOn { .. }) && velocity > 0;

            if on {
                let note = NoteInfo {
                    channel: channel.as_int(),
                    velocity,
                    pulses,
                };
                active_notes.insert(key, note);
            }
        }
    }

    notes
}

fn build_events(
    track_id: usize,
    track_color_id: usize,
    tempo_track: &TempoTrack,
    track_events: &[TrackEvent],
) -> (Vec<MidiEvent>, Vec<ProgramEvent>, bool, bool) {
    let mut programs = Vec::new();
    let mut has_drums = false;
    let mut has_other_than_drums = false;

    let mut pulses: u64 = 0;
    let events = track_events
        .iter()
        .filter_map(|event| {
            pulses += event.delta.as_int() as u64;
            match event.kind {
                TrackEventKind::Midi { channel, message } => {
                    let timestamp = tempo_track.pulses_to_duration(pulses);

                    let message = match message {
                        midly::MidiMessage::NoteOn { key, vel } => {
                            if channel == 9 || channel == 15 {
                                has_drums = true;
                            } else {
                                has_other_than_drums = true;
                            }

                            if vel.as_int() > 0 {
                                message
                            } else {
                                midly::MidiMessage::NoteOff { key, vel }
                            }
                        }
                        midly::MidiMessage::ProgramChange { program } => {
                            programs.push(ProgramEvent {
                                timestamp,
                                channel: channel.as_int(),
                                program: program.as_int(),
                            });
                            message
                        }
                        message => message,
                    };

                    Some(MidiEvent {
                        channel: channel.as_int(),
                        timestamp,
                        message,
                        track_id,
                        track_color_id,
                    })
                }
                _ => None,
            }
        })
        .collect();

    (events, programs, has_drums, has_other_than_drums)
}
