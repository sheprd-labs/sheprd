//! Synthesized semantic audio cues played on a background thread through Rodio.
//! Keeps the UI thread non-blocking and needs no bundled audio asset files.

use rodio::{DeviceSinkBuilder, Player, buffer::SamplesBuffer};
use sheprd_control_protocol::protocol::modal::CueKind;
use std::f32::consts::TAU;

const SAMPLE_RATE: u32 = 44_100;

/// Plays `cue` asynchronously on a detached worker thread, respecting
/// `HERDR_DISABLE_SOUND`, `NEXTEST`, and `volume_percent`.
pub(crate) fn play_async(cue: CueKind, volume_percent: u8) {
    if crate::sound::muted() || volume_percent == 0 {
        return;
    }
    let level = f32::from(volume_percent.min(crate::config::SoundConfig::MAX_VOLUME)) / 100.;
    let gain = level * level;
    let _ = std::thread::Builder::new()
        .name("sheprd-cue".into())
        .spawn(move || play_blocking(cue, gain));
}

fn play_blocking(cue: CueKind, gain: f32) {
    let samples = synthesize_cue(cue);
    if samples.is_empty() {
        return;
    }
    let Ok(builder) = DeviceSinkBuilder::from_default_device() else {
        return;
    };
    let Ok(mut output) = builder.open_sink_or_fallback() else {
        return;
    };
    output.log_on_drop(false);
    let player = Player::connect_new(output.mixer());
    player.set_volume(gain);
    let (Some(channels), Some(rate)) = (
        std::num::NonZero::new(1u16),
        std::num::NonZero::new(SAMPLE_RATE),
    ) else {
        return;
    };
    player.append(SamplesBuffer::new(channels, rate, samples));
    player.sleep_until_end();
}

/// Pure PCM synthesizer for the 5 semantic interaction cues (mono, 44.1 kHz).
pub(crate) fn synthesize_cue(cue: CueKind) -> Vec<f32> {
    let notes: &[(f32, f32, f32, f32)] = match cue {
        // (start_s, duration_s, freq_hz, amplitude)
        CueKind::Yubikey => &[(0.0, 0.09, 240.0, 0.35), (0.14, 0.11, 320.0, 0.40)],
        CueKind::Decision => &[(0.0, 0.16, 523.25, 0.26), (0.12, 0.22, 659.25, 0.26)],
        CueKind::Secret => &[(0.0, 0.13, 784.0, 0.22), (0.09, 0.20, 1046.5, 0.22)],
        CueKind::Countdown => &[(0.0, 0.05, 880.0, 0.24), (0.28, 0.05, 880.0, 0.24)],
        CueKind::Done => &[
            (0.0, 0.22, 523.25, 0.22),
            (0.07, 0.22, 659.25, 0.22),
            (0.14, 0.28, 783.99, 0.24),
        ],
    };
    let total_secs = notes
        .iter()
        .map(|(start, dur, _, _)| start + dur)
        .fold(0.0_f32, f32::max);
    let len = (total_secs * SAMPLE_RATE as f32).ceil() as usize;
    let mut out = vec![0.0_f32; len];
    for &(start, dur, freq, amp) in notes {
        let start_i = (start * SAMPLE_RATE as f32) as usize;
        let count = (dur * SAMPLE_RATE as f32) as usize;
        for i in 0..count {
            let idx = start_i + i;
            if idx >= out.len() {
                break;
            }
            let t = i as f32 / SAMPLE_RATE as f32;
            let attack = (t / 0.012).min(1.0);
            let release = ((dur - t) / 0.035).clamp(0.0, 1.0);
            let env = attack * release;
            let wave = (TAU * freq * t).sin();
            out[idx] = (out[idx] + wave * amp * env).clamp(-0.95, 0.95);
        }
    }
    out
}
