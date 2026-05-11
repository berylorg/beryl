# Transcript Image Markers

## Markdown-Parsed Marker Overlay Was Too Brittle

During Phase 11 implementation, the first rendering path overlaid structured image marker ranges after parsing the user fragment as ordinary Markdown. That kept the data source typed, but it still allowed Markdown syntax to consume the marker display text. For example, `[A](url)` could parse as link text and code blocks bypassed inline marker rendering entirely.

The corrected approach keeps image marker records structured and replaces only their known display ranges with same-length Markdown-neutral placeholders before parsing. The renderer maps the structured marker ranges back onto the rendered text and displays the marker as `[A]`, so literal user-authored `[A]` text is still not treated as an attachment, while typed markers do not become Markdown links or disappear inside code/fallback blocks.

## Standalone Label-Record Assumption Was Too Narrow

During Phase 12 live testing, transcript reload showed `Image A:` leaking before a reconstructed marker even though Beryl consumed standalone `Text("Image A:")` plus image pairs. The failure mode is adjacent backend text merging: Beryl submits ordinary text and generated label text as separate records, but historical data may return them as one text record ending with `Image A:` before the image.

The corrected reconstruction treats a generated `Image <label>:` suffix as metadata only when the immediately following backend record is an image. The text prefix remains user-authored display text, and the suffix plus image become one typed marker. This keeps the repair bounded to structured image-adjacent backend records rather than parsing arbitrary marker-shaped plaintext into attachments.

## Immediate-Next-Image Assumption Was Still Too Narrow

Further Phase 12 live testing showed the backend can return the text surrounding a pasted image as one text record and place the local image record later in the same user input. The visible symptom was `Image B:` leaking at the original marker position while the later image record fell back to count-based label `A` at the end of the text.

The corrected reconstruction must treat Beryl-generated label anchors as pending structured anchors within the same backend user input, and bind the next unbound image record to the earliest pending anchor. This remains bounded to Beryl-generated `Image <label>:` text plus an actual image record in the same content array; literal marker-shaped text without an image record remains ordinary transcript text.
