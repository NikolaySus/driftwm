use std::time::{Duration, Instant};

use smithay::desktop::{PopupManager, WindowSurfaceType, layer_map_for_output};
use smithay::output::Output;

use super::{DriftWm, output_logical_size};

#[derive(Default)]
pub struct PanelVisibility {
    visible: bool,
    hide_at: Option<Instant>,
}

impl PanelVisibility {
    fn update(
        &mut self,
        now: Instant,
        reveal: bool,
        keep: bool,
        locked: bool,
        delay: Duration,
    ) -> bool {
        let was_visible = self.visible;
        if locked {
            self.visible = false;
            self.hide_at = None;
        } else if reveal || (self.visible && keep) {
            self.visible = true;
            self.hide_at = None;
        } else if self.visible {
            let deadline = *self.hide_at.get_or_insert(now + delay);
            if now >= deadline {
                self.visible = false;
                self.hide_at = None;
            }
        }
        was_visible != self.visible
    }
}

impl DriftWm {
    pub(crate) fn panel_is_hidden(&self, output: &Output, namespace: &str) -> bool {
        self.config.panel.autohide
            && namespace == self.config.panel.namespace
            && (self.session_lock.is_locked()
                || !self.panel_visibility.get(output).is_some_and(|s| s.visible))
    }

    pub(crate) fn update_panel_visibility(&mut self, now: Instant) {
        if !self.config.panel.autohide {
            return;
        }
        let locked = self.session_lock.is_locked();
        let super_held = self
            .seat
            .get_keyboard()
            .is_some_and(|k| k.modifier_state().logo);
        let active = self.active_output();
        let screen = self.seat.get_pointer().map(|p| {
            driftwm::canvas::canvas_to_screen(
                driftwm::canvas::CanvasPos(p.current_location()),
                self.camera(),
                self.zoom(),
            )
            .0
        });
        let outputs: Vec<_> = self.space.outputs().cloned().collect();
        self.panel_visibility.retain(|o, _| outputs.contains(o));
        let mut changed = false;
        for output in outputs {
            let pointer = screen
                .filter(|_| active.as_ref() == Some(&output) && !self.pointer_constraint_locked());
            let size = output_logical_size(&output);
            let at_edge = pointer.is_some_and(|p| {
                p.x >= 0.0
                    && p.x < size.w as f64
                    && p.y >= 0.0
                    && p.y < f64::from(self.config.panel.edge_size.min(32))
            });
            let map = layer_map_for_output(&output);
            let mut keep = false;
            for surface in map
                .layers()
                .filter(|s| s.namespace() == self.config.panel.namespace)
            {
                if let Some(geo) = map.layer_geometry(surface) {
                    keep |= pointer.is_some_and(|p| {
                        surface
                            .surface_under(p - geo.loc.to_f64(), WindowSurfaceType::ALL)
                            .is_some()
                    });
                }
                // A tray menu may extend past the bar or temporarily own a grab.
                keep |= PopupManager::popups_for_surface(surface.wl_surface())
                    .next()
                    .is_some();
            }
            drop(map);
            let visibility = self.panel_visibility.entry(output).or_default();
            // Preserve a pointer grab begun on the panel until its button release.
            keep |= visibility.visible && !self.held_buttons.is_empty();
            changed |= visibility.update(
                now,
                super_held || at_edge,
                keep,
                locked,
                Duration::from_millis(u64::from(self.config.panel.hide_delay_ms)),
            );
        }
        if changed {
            self.mark_all_dirty();
            self.pending_pointer_resync = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_panel_does_not_reveal_from_hovering_its_old_rectangle() {
        let mut panel = PanelVisibility::default();
        assert!(!panel.update(Instant::now(), false, true, false, Duration::ZERO));
        assert!(!panel.visible);
    }

    #[test]
    fn reveal_and_interaction_cancel_the_pending_hide() {
        let mut panel = PanelVisibility::default();
        let now = Instant::now();
        let delay = Duration::from_millis(300);
        assert!(panel.update(now, true, false, false, delay));
        assert!(!panel.update(now, false, false, false, delay));
        assert!(!panel.update(now + delay / 2, false, true, false, delay));
        assert!(!panel.update(now + delay, false, false, false, delay));
        assert!(panel.visible);
        assert!(panel.update(now + delay * 2, false, false, false, delay));
        assert!(!panel.visible);
    }

    #[test]
    fn lock_hides_immediately_even_while_super_or_menu_is_active() {
        let mut panel = PanelVisibility::default();
        let now = Instant::now();
        panel.update(now, true, false, false, Duration::from_secs(1));
        assert!(panel.update(now, true, true, true, Duration::from_secs(1)));
        assert!(!panel.visible);
        assert!(panel.hide_at.is_none());
    }
}
