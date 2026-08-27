# Status Line GUI

## Status Line

Mount-into: main-window.status-line

This feature configures the built-in segmented status bar; it does not define a new widget. The arrangement has three left-to-right segments: content-sized `M/R`, flexible Context, and content-sized Turn. Turn reserves enough content width for `compacting`, its longest state label.

`M/R` presents the model and reasoning readout. Context presents context space, applicable short-window and weekly rate-limit readouts, then `I`, `IC`, and `O` selected-thread token counters in that order. Turn presents the latest turn-state readout. The model/reasoning, Context, and Turn segments retain the feature behavior and interaction availability defined in the status-line design contract.
