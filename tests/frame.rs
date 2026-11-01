//! Exercise the actual injected frame helper with device adapters replaced.
extern crate self as horizon_client_render;

pub mod official_speak_audio {
    use std::cell::RefCell;
    #[derive(Default)]
    pub struct Output {
        pub capture: bool,
        pub played: Vec<Vec<i16>>,
        pub stops: usize,
    }
    thread_local! { pub static OUTPUT: RefCell<Output> = RefCell::default(); }
    pub fn set_capture_enabled(enabled: bool) -> Result<(), String> {
        OUTPUT.with_borrow_mut(|s| s.capture = enabled);
        Ok(())
    }
    pub fn take_frames() -> Vec<Vec<i16>> {
        Vec::new()
    }
    pub fn enqueue_playback(pcm: &[i16], _: f32) -> Result<(), String> {
        OUTPUT.with_borrow_mut(|s| s.played.push(pcm.to_vec()));
        Ok(())
    }
    pub fn stop() {
        OUTPUT.with_borrow_mut(|s| {
            s.capture = false;
            s.stops += 1;
        });
    }
}

mod game {
    pub use horizon_mod_speak as official_speak;
    pub struct Game<'a> {
        pub world: bool,
        pub playing: bool,
        pub enabled: bool,
        pub marker: std::marker::PhantomData<&'a ()>,
    }
    impl Game<'_> {
        pub fn native_on(&self, _: &str) -> bool {
            self.enabled
        }
        pub fn native_window(&self) -> bool {
            true
        }
        pub fn native_player(&self) -> Option<()> {
            self.world.then_some(())
        }
        pub fn native_playing(&self) -> bool {
            self.playing
        }
        pub fn native_setting(&self, _: &str, _: &str) -> Option<String> {
            None
        }
        pub fn native_net_send_to_server(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
            Ok(())
        }
        pub fn frame(&mut self) {
            speak_frame(self);
        }
    }
    include!("../mixins/speak/frame.rs");
}

#[test]
fn menu_keeps_received_pcm_but_releases_capture_until_a_new_press() {
    use horizon_mod_speak as speak;
    use official_speak_audio::OUTPUT;
    speak::closed();
    speak::note_capture(false);
    let mut game = game::Game {
        world: true,
        playing: true,
        enabled: true,
        marker: std::marker::PhantomData,
    };
    speak::key(true, true, |_| None);
    game.frame();
    OUTPUT.with_borrow(|s| assert!(s.capture));
    let pcm = vec![1234; 960];
    speak::queue_playback(speak::SoundPacket {
        sender: 7,
        sender_name: "Nearby".into(),
        dimension: "minecraft:overworld".into(),
        position: [0.0; 3],
        sequence: 1,
        data: speak::pcm_bytes(&pcm),
    });
    game.playing = false; // Chat/inventory overlays the same connected world.
    game.frame();
    OUTPUT.with_borrow(|s| {
        assert!(!s.capture);
        assert_eq!(s.played, vec![pcm]);
        assert_eq!(s.stops, 0);
    });
    assert!(!speak::talking());
    game.playing = true;
    game.frame();
    OUTPUT.with_borrow(|s| assert!(!s.capture));
    speak::key(true, true, |_| None);
    game.frame();
    OUTPUT.with_borrow(|s| assert!(s.capture));
    game.world = false;
    game.frame();
    OUTPUT.with_borrow(|s| {
        assert!(!s.capture);
        assert_eq!(s.stops, 1);
    });
    assert!(!speak::talking());
}
