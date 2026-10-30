`nih_plug_egui` from nih-plug rev de421011 (ISC licence), changed only to make
`EguiState::set_requested_size()` public so the editor can offer window size presets, to take
`nih_plug` from the same git revision instead of a sibling path, and to fix a resize bug: the new
size was stored only after asking the host to resize, so the host was given the old size
(`src/editor.rs`), and the content was resized to the logical size given as physical pixels, so
on a Retina display it covered a quarter of the window.
