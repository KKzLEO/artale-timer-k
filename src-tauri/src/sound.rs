use rodio::{OutputStream, Sink, Source};
use std::time::Duration;

/// Play a short beep sound for warnings (higher pitch, shorter)
pub fn play_warning_beep() {
    std::thread::spawn(|| {
        if let Ok((_stream, handle)) = OutputStream::try_default() {
            if let Ok(sink) = Sink::try_new(&handle) {
                let source = rodio::source::SineWave::new(880.0)
                    .take_duration(Duration::from_millis(100))
                    .amplify(0.3);
                sink.append(source);
                sink.sleep_until_end();
            }
        }
    });
}

/// Play a softer reminder for buff expiry (gentler tone)
pub fn play_buff_expired_beep() {
    std::thread::spawn(|| {
        if let Ok((_stream, handle)) = OutputStream::try_default() {
            if let Ok(sink) = Sink::try_new(&handle) {
                // Three quick ascending chimes
                let chime1 = rodio::source::SineWave::new(523.0) // C5
                    .take_duration(Duration::from_millis(100))
                    .amplify(0.25);
                let gap = rodio::source::Zero::<f32>::new(1, 44100)
                    .take_duration(Duration::from_millis(60));
                let chime2 = rodio::source::SineWave::new(659.0) // E5
                    .take_duration(Duration::from_millis(100))
                    .amplify(0.25);
                let gap2 = rodio::source::Zero::<f32>::new(1, 44100)
                    .take_duration(Duration::from_millis(60));
                let chime3 = rodio::source::SineWave::new(784.0) // G5
                    .take_duration(Duration::from_millis(120))
                    .amplify(0.3);
                sink.append(chime1);
                sink.append(gap);
                sink.append(chime2);
                sink.append(gap2);
                sink.append(chime3);
                sink.sleep_until_end();
            }
        }
    });
}

/// Play a softer warning for buff approaching expiry
pub fn play_buff_warning_beep() {
    std::thread::spawn(|| {
        if let Ok((_stream, handle)) = OutputStream::try_default() {
            if let Ok(sink) = Sink::try_new(&handle) {
                let source = rodio::source::SineWave::new(698.0) // F5, softer than boss warning
                    .take_duration(Duration::from_millis(80))
                    .amplify(0.2);
                sink.append(source);
                sink.sleep_until_end();
            }
        }
    });
}

/// Play a more urgent beep for expiry (lower pitch, longer)
pub fn play_expired_beep() {
    std::thread::spawn(|| {
        if let Ok((_stream, handle)) = OutputStream::try_default() {
            if let Ok(sink) = Sink::try_new(&handle) {
                // Two short beeps
                let beep1 = rodio::source::SineWave::new(660.0)
                    .take_duration(Duration::from_millis(150))
                    .amplify(0.4);
                let silence = rodio::source::Zero::<f32>::new(1, 44100)
                    .take_duration(Duration::from_millis(80));
                let beep2 = rodio::source::SineWave::new(660.0)
                    .take_duration(Duration::from_millis(150))
                    .amplify(0.4);
                sink.append(beep1);
                sink.append(silence);
                sink.append(beep2);
                sink.sleep_until_end();
            }
        }
    });
}
