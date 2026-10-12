# Fenced Picker Pointer Focus

The thread-root picker contract keeps inspection focus and traversal available while an external
command fences mutations. Its disabled search input is therefore represented by a focusable owner
container. That container assigned search focus on primary press but allowed the event to bubble
to the underlying shell, which reassigned focus before dispatch finished.

The existing fenced-search inspection regression reproduced this in runs
`a8887714-1def-4c26-b7fa-6129d05f6e0d` and `bfcf2b0f-4d6b-4400-a2eb-f8f9995cbc96`.
Drawing a fresh frame and moving the pointer before clicking did not correct it. Diagnostic run
`f10a4a86-abfa-4d3d-b795-867e08e2a7c7` proved the search listener ran and assigned focus ID 18,
but dispatch ended on focus ID 9. The input's text, selection and marked-text assertions had passed.

Consume the primary press after assigning inspection focus when the external fence is present.
Keep input mutation disabled and preserve ordinary unfenced handling. The existing regression
checks rejected edit/paste/cut/undo/redo, preserved text/selection/marked text, inspection traversal,
unavailable-command focus and pointer return to the search field. No dependency behavior or new
readonly input API is required.
