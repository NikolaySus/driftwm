use driftwm::animation_clock::{AnimationSettings, Easing};
use driftwm::config::Config;

#[test]
fn background_animation_defaults_and_inheritance() {
    let config = Config::from_toml("").unwrap();
    assert_eq!(config.background.animation, AnimationSettings::default());
    let config = Config::from_toml("[background]\nanimation_speed = 2.0").unwrap();
    assert_eq!(config.background.animation.speed, 2.0);
    assert_eq!(config.background.animation.lock_speed, 2.0);
}

#[test]
fn background_animation_options_and_invalid_values() {
    for (name, easing) in [
        ("linear", Easing::Linear),
        ("ease-in", Easing::EaseIn),
        ("ease-out", Easing::EaseOut),
        ("ease-in-out", Easing::EaseInOut),
    ] {
        let config = Config::from_toml(&format!("[background]\nanimation_speed = 0.0\nlock_animation_speed = 0.5\nspeed_transition_duration_ms = 0\nspeed_transition_easing = \"{name}\"\n")).unwrap();
        assert_eq!(
            config.background.animation,
            AnimationSettings {
                speed: 0.0,
                lock_speed: 0.5,
                duration_ms: 0,
                easing
            }
        );
    }
    for bad in ["-1.0", "nan", "inf", "-inf"] {
        let (config, warnings) = Config::from_toml_collect(&format!("[background]\nanimation_speed = {bad}\nlock_animation_speed = {bad}\nspeed_transition_easing = \"invalid\"\n")).unwrap();
        assert_eq!(config.background.animation, AnimationSettings::default());
        assert_eq!(warnings.len(), 3);
    }
}
