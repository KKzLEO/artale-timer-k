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
