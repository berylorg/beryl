use std::time::{Duration, Instant};

use gpui::{AsyncApp, Context, Task, WeakEntity};

use crate::shell::{SCROLLBAR_FADE_DELAY, SCROLLBAR_FADE_DURATION};

use super::ChecklistSidebarPanel;

pub(super) struct SidebarScrollbarActivity {
    generation: u64,
    last_activity_at: Option<Instant>,
    transition: Option<SidebarScrollbarTransition>,
    animation_task: Option<Task<()>>,
}

struct SidebarScrollbarTransition {
    started_at: Instant,
    from_opacity: f32,
    to_opacity: f32,
}

impl Default for SidebarScrollbarActivity {
    fn default() -> Self {
        Self {
            generation: 0,
            last_activity_at: None,
            transition: None,
            animation_task: None,
        }
    }
}

impl SidebarScrollbarActivity {
    fn record_activity(&mut self) -> u64 {
        let now = Instant::now();
        let current_opacity = self.opacity_at(now);
        self.generation = self.generation.saturating_add(1);
        self.last_activity_at = Some(now);
        self.transition = if current_opacity >= (1.0 - f32::EPSILON) {
            None
        } else {
            Some(SidebarScrollbarTransition {
                started_at: now,
                from_opacity: current_opacity,
                to_opacity: 1.0,
            })
        };
        self.animation_task = None;
        self.generation
    }

    fn extend_visible_activity(&mut self, now: Instant) -> Option<u64> {
        if self.transition.is_some() || self.opacity_at(now) < (1.0 - f32::EPSILON) {
            return None;
        }

        self.last_activity_at = Some(now);
        self.animation_task.is_none().then_some(self.generation)
    }

    pub(super) fn opacity(&self) -> f32 {
        self.opacity_at(Instant::now())
    }

    fn opacity_at(&self, now: Instant) -> f32 {
        if let Some(transition) = &self.transition {
            transition.opacity(now)
        } else if self.last_activity_at.is_some() {
            1.0
        } else {
            0.0
        }
    }

    pub(super) fn is_animating(&self) -> bool {
        self.transition
            .as_ref()
            .is_some_and(|transition| transition.is_active(Instant::now()))
    }
}

impl SidebarScrollbarTransition {
    fn duration(&self) -> Duration {
        let delta = (self.to_opacity - self.from_opacity).abs();
        if delta <= f32::EPSILON {
            return Duration::ZERO;
        }

        let duration = SCROLLBAR_FADE_DURATION.mul_f32(delta);
        if duration.is_zero() {
            Duration::from_millis(1)
        } else {
            duration
        }
    }

    fn progress(&self, now: Instant) -> f32 {
        let duration = self.duration();
        if duration.is_zero() {
            return 1.0;
        }

        let elapsed = now.saturating_duration_since(self.started_at);
        (elapsed.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    fn opacity(&self, now: Instant) -> f32 {
        let progress = self.progress(now);
        let eased_progress = progress * progress * (3.0 - (2.0 * progress));
        self.from_opacity + ((self.to_opacity - self.from_opacity) * eased_progress)
    }

    fn is_active(&self, now: Instant) -> bool {
        self.progress(now) < 1.0
    }

    fn remaining_duration(&self, now: Instant) -> Option<Duration> {
        let duration = self.duration();
        if duration.is_zero() {
            return None;
        }

        let elapsed = now.saturating_duration_since(self.started_at);
        duration.checked_sub(elapsed)
    }
}

impl ChecklistSidebarPanel {
    pub(super) fn scrollbar_opacity(&self) -> f32 {
        self.scrollbar_activity.opacity()
    }

    pub(super) fn scrollbar_animating(&self) -> bool {
        self.scrollbar_activity.is_animating()
    }

    fn note_scrollbar_activity(&mut self, cx: &mut Context<Self>) {
        let generation = self.scrollbar_activity.record_activity();
        self.schedule_scrollbar_animation(generation, cx);
        cx.notify();
    }

    fn note_scrollbar_activity_without_redundant_notify(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let mut extended_without_visual_change = false;
        let mut generation_to_schedule = None;
        if self.scrollbar_activity.transition.is_none()
            && self.scrollbar_activity.opacity_at(now) >= (1.0 - f32::EPSILON)
        {
            generation_to_schedule = self.scrollbar_activity.extend_visible_activity(now);
            extended_without_visual_change = true;
        }

        if let Some(generation) = generation_to_schedule {
            self.schedule_scrollbar_animation(generation, cx);
        } else if !extended_without_visual_change {
            self.note_scrollbar_activity(cx);
        }
    }

    fn schedule_scrollbar_animation(&mut self, generation: u64, cx: &mut Context<Self>) {
        let Some(next_delay) = self.next_scrollbar_animation_delay(generation) else {
            return;
        };

        let animation_task = cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                cx.background_executor().timer(next_delay).await;
                let _ = view.update(&mut cx, |view: &mut Self, cx: &mut Context<Self>| {
                    view.advance_scrollbar_animation(generation, cx);
                });
            }
        });

        self.scrollbar_activity.animation_task = Some(animation_task);
    }

    fn next_scrollbar_animation_delay(&self, generation: u64) -> Option<Duration> {
        let now = Instant::now();
        if self.scrollbar_activity.generation != generation {
            return None;
        }

        if let Some(transition) = &self.scrollbar_activity.transition {
            return transition.remaining_duration(now);
        }

        let last_activity_at = self.scrollbar_activity.last_activity_at?;
        let fade_deadline = last_activity_at + SCROLLBAR_FADE_DELAY;
        (now < fade_deadline).then_some(fade_deadline.saturating_duration_since(now))
    }

    fn advance_scrollbar_animation(&mut self, generation: u64, cx: &mut Context<Self>) {
        if self.scrollbar_activity.generation != generation {
            return;
        }
        self.scrollbar_activity.animation_task = None;

        let now = Instant::now();
        let mut should_notify = false;
        if let Some(transition) = &self.scrollbar_activity.transition
            && !transition.is_active(now)
        {
            let target_opacity = transition.to_opacity;
            self.scrollbar_activity.transition = None;
            should_notify = true;
            if target_opacity <= 0.0 {
                self.scrollbar_activity.last_activity_at = None;
            }
        }

        if self.scrollbar_activity.transition.is_none() {
            let Some(last_activity_at) = self.scrollbar_activity.last_activity_at else {
                if should_notify {
                    cx.notify();
                }
                return;
            };
            let fade_deadline = last_activity_at + SCROLLBAR_FADE_DELAY;
            if now >= fade_deadline {
                let current_opacity = self.scrollbar_activity.opacity_at(now);
                if current_opacity <= 0.0 {
                    self.scrollbar_activity.last_activity_at = None;
                } else {
                    self.scrollbar_activity.transition = Some(SidebarScrollbarTransition {
                        started_at: now,
                        from_opacity: current_opacity,
                        to_opacity: 0.0,
                    });
                    should_notify = true;
                }
            }
        }

        if should_notify {
            cx.notify();
        }

        if self.scrollbar_activity.generation == generation {
            self.schedule_scrollbar_animation(generation, cx);
        }
    }

    pub(super) fn note_scrollbar_motion(
        &mut self,
        _: &gpui::MouseMoveEvent,
        _: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity_without_redundant_notify(cx);
    }

    pub(super) fn note_scrollbar_scroll(
        &mut self,
        _: &gpui::ScrollWheelEvent,
        _: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(cx);
    }
}
