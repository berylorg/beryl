# Transcript Presentation Failures

## 2026-05-03: Steering Fragments Were Hoisted Before Assistant Output

During live testing, active-turn steering delivered input successfully but rendered the steering fragment beside the turn's original user prompt. The assistant had already produced parent conversation output, so moving the later user fragment upward made the transcript order misleading.

The invalid approach was modeling all same-turn user input fragments as a turn-level list and rendering that whole list before every assistant item. That preserved distinct fragments, but it lost the accepted narrative position of fragments submitted through active-turn steering.

The course adjustment is to keep a per-turn narrative order projection that includes both user input fragments and parent narrative items. Initial and queued fragments still start a turn in order, while live steering fragments append at the current transcript tail. Historical loading preserves backend item order for user-message items and assistant narrative items.
