//! Alarm sounds, generated as tones (no sound files): the 2182 kHz radiotelephone alarm signal,
//! the DSC alarm, NAVTEX / Inmarsat-C alarm beeps and the SART's short beep.

use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::time::Duration;

const RATE: u32 = 44_100;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tone {
    /// Two-tone alarm signal: 2200 Hz and 1300 Hz, 250 ms each.
    TwoTone,
    /// DSC distress / urgency alarm: fast 2100 / 1300 Hz warble, with a pause each second.
    Dsc,
    /// 1 kHz beeps, half a second on and off.
    Beeps,
    /// One short 3 kHz beep.
    Chirp,
}

struct Gen {
    tone: Tone,
    n: u64,
    phase: f32,
}

impl Iterator for Gen {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let t = self.n as f32 / RATE as f32;
        self.n += 1;
        let (f, on) = match self.tone {
            Tone::TwoTone => (if (t / 0.25) as u64 % 2 == 0 { 2200.0 } else { 1300.0 }, true),
            Tone::Dsc => (if (t / 0.12) as u64 % 2 == 0 { 2100.0 } else { 1300.0 }, t % 1.0 < 0.75),
            Tone::Beeps => (1000.0, t % 1.0 < 0.5),
            Tone::Chirp => (3000.0, true),
        };
        // Phase accumulates so the frequency changes without clicks.
        self.phase = (self.phase + f / RATE as f32) % 1.0;
        Some(if on { (self.phase * std::f32::consts::TAU).sin() * 0.2 } else { 0.0 })
    }
}

impl Source for Gen {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub struct Sound {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    sink: Sink,
    playing: Option<Tone>,
}

impl Sound {
    /// None when there is no audio device.
    pub fn new() -> Option<Sound> {
        let (stream, handle) = OutputStream::try_default().ok()?;
        let sink = Sink::try_new(&handle).ok()?;
        Some(Sound { _stream: stream, handle, sink, playing: None })
    }

    /// Plays `tone` without end (None: silence); does nothing while it is already playing.
    pub fn set(&mut self, tone: Option<Tone>) {
        if tone == self.playing {
            return;
        }
        self.sink.stop();
        if let Ok(sink) = Sink::try_new(&self.handle) {
            self.sink = sink;
        }
        if let Some(tone) = tone {
            self.sink.append(Gen { tone, n: 0, phase: 0.0 });
        }
        self.playing = tone;
    }

    /// A short beep over whatever else plays.
    pub fn chirp(&self) {
        let _ = self.handle.play_raw(Gen { tone: Tone::Chirp, n: 0, phase: 0.0 }.take_duration(Duration::from_millis(120)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_tone_alternates_every_quarter_second() {
        // Count zero crossings in the first and second 250 ms: about 2 x 2200 x 0.25 and 2 x 1300 x 0.25.
        let s: Vec<f32> = Gen { tone: Tone::TwoTone, n: 0, phase: 0.0 }.take(RATE as usize / 2).collect();
        let crossings = |w: &[f32]| w.windows(2).filter(|p| (p[0] < 0.0) != (p[1] < 0.0)).count();
        let q = RATE as usize / 4;
        assert!((crossings(&s[..q]) as i32 - 1100).abs() < 10);
        assert!((crossings(&s[q..]) as i32 - 650).abs() < 10);
    }
}
