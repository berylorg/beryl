use super::{InputSpec, generator::Part};

#[derive(Clone, Copy)]
pub(super) enum ContentFlavor {
    Request,
    Echo,
}

pub(super) struct ContentBytes {
    spec: InputSpec,
    flavor: ContentFlavor,
    stage: ContentStage,
    part: Part,
    marker_ordinal: u64,
    final_text: bool,
}

#[derive(Clone, Copy)]
enum ContentStage {
    Start,
    TextPattern,
    LabelPrefix,
    Label,
    TextToImage,
    ImagePath,
    AfterImage,
    NextText,
    FinalSuffix,
    Done,
}

impl ContentBytes {
    pub(super) const fn new(spec: InputSpec, flavor: ContentFlavor) -> Self {
        Self {
            spec,
            flavor,
            stage: ContentStage::Start,
            part: Part::empty(),
            marker_ordinal: 1,
            final_text: false,
        }
    }

    fn schedule(&mut self) {
        match self.stage {
            ContentStage::Start => {
                self.part = Part::bytes(b"[{\"type\":\"text\",\"text\":\"");
                if self.spec.marker_count().is_none() {
                    self.final_text = true;
                }
                self.stage = ContentStage::TextPattern;
            }
            ContentStage::TextPattern => {
                self.part = Part::escaped_pattern(self.spec.repetitions().get());
                self.stage = if self.final_text {
                    ContentStage::FinalSuffix
                } else {
                    ContentStage::LabelPrefix
                };
            }
            ContentStage::LabelPrefix => {
                self.part = Part::bytes(b"Image ");
                self.stage = ContentStage::Label;
            }
            ContentStage::Label => {
                self.part = Part::image_label(self.marker_ordinal);
                self.stage = ContentStage::TextToImage;
            }
            ContentStage::TextToImage => {
                self.part = Part::bytes(match self.flavor {
                    ContentFlavor::Request => {
                        b":\"},{\"type\":\"localImage\",\"path\":\""
                    }
                    ContentFlavor::Echo => b":\",\"text_elements\":[]},{\"type\":\"localImage\",\"detail\":null,\"path\":\"",
                });
                self.stage = ContentStage::ImagePath;
            }
            ContentStage::ImagePath => {
                self.part = Part::escaped_path();
                self.stage = ContentStage::AfterImage;
            }
            ContentStage::AfterImage => {
                self.part = Part::bytes(b"\"}");
                self.stage = ContentStage::NextText;
            }
            ContentStage::NextText => {
                let marker_count = self
                    .spec
                    .marker_count()
                    .expect("only marker-aware content reaches image advancement")
                    .get();
                if self.marker_ordinal < marker_count {
                    self.marker_ordinal += 1;
                } else {
                    self.final_text = true;
                }
                self.part = Part::bytes(b",{\"type\":\"text\",\"text\":\"");
                self.stage = ContentStage::TextPattern;
            }
            ContentStage::FinalSuffix => {
                self.part = Part::bytes(match self.flavor {
                    ContentFlavor::Request => b"\"}]",
                    ContentFlavor::Echo => b"\",\"text_elements\":[]}]",
                });
                self.stage = ContentStage::Done;
            }
            ContentStage::Done => {}
        }
    }
}

impl Iterator for ContentBytes {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(byte) = self.part.next(&self.spec) {
                return Some(byte);
            }
            if matches!(self.stage, ContentStage::Done) {
                return None;
            }
            self.schedule();
        }
    }
}
