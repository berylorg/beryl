//! Deterministic recovery-history shapes and exact expected sequence facts.

use beryl_model::{
    RecoveryItemSequenceAccumulator, RecoveryItemSequenceDigest, RecoveryItemSequenceRole,
    SyndicThreadId,
};
use syndic_storage::{
    CasRepresentedPrefixProof, RecoveryAssembly, RecoveryProjection, RecoveryProjectionRequest,
    SelectedPathProof,
};

use crate::syndic::Fixture;

pub const PRODUCT_UTF8_LIMIT: u64 = 262_144;
pub const MODEL_BOUNDARY_TOKENS: u64 = PRODUCT_UTF8_LIMIT * 2;

const FRAGMENTED_LONG_ITEM_BYTES: usize = 65_537;
const FRAGMENTED_SHORT_ITEM_BYTES: usize = 65_535;
const DEEP_TURNS: usize = 80;
const DEEP_ITEMS: usize = DEEP_TURNS * 2;
const DEEP_ITEM_BASE_BYTES: usize = PRODUCT_UTF8_LIMIT as usize / DEEP_ITEMS;
const DEEP_LONG_ITEM_COUNT: usize = PRODUCT_UTF8_LIMIT as usize % DEEP_ITEMS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryRole {
    User,
    Assistant,
}

pub struct HistoryItem {
    role: HistoryRole,
    text: String,
}

pub struct HistorySpec {
    items: Vec<HistoryItem>,
    expected_pages: u64,
}

#[derive(Clone, Copy)]
pub struct InstalledHistory {
    pub thread: SyndicThreadId,
    pub selected_path: SelectedPathProof,
    pub completed_prefix: CasRepresentedPrefixProof,
}

impl HistorySpec {
    /// Four exact-limit items. Each user item places a two-byte scalar across a
    /// 64 KiB page boundary, so its first page must retreat by one byte.
    #[must_use]
    pub fn fragmented_limit() -> Self {
        let mut items = Vec::with_capacity(4);
        for turn in 0..2 {
            items.push(HistoryItem {
                role: HistoryRole::User,
                text: split_scalar_text(&format!("fragment-user-{turn}|")),
            });
            items.push(HistoryItem {
                role: HistoryRole::Assistant,
                text: padded_ascii(
                    &format!("fragment-assistant-{turn}|"),
                    FRAGMENTED_SHORT_ITEM_BYTES,
                    b'a' + u8::try_from(turn).unwrap(),
                ),
            });
        }
        Self::checked(items, 6)
    }

    /// One hundred sixty alternating items no larger than 1,639 bytes at the
    /// exact product byte limit.
    #[must_use]
    pub fn deep_limit() -> Self {
        let mut items = Vec::with_capacity(DEEP_ITEMS);
        for turn in 0..DEEP_TURNS {
            let user_index = turn * 2;
            let assistant_index = user_index + 1;
            items.push(HistoryItem {
                role: HistoryRole::User,
                text: padded_ascii(
                    &format!("deep-user-{turn:02}|"),
                    deep_item_bytes(user_index),
                    b'u',
                ),
            });
            items.push(HistoryItem {
                role: HistoryRole::Assistant,
                text: padded_ascii(
                    &format!("deep-assistant-{turn:02}|"),
                    deep_item_bytes(assistant_index),
                    b'a',
                ),
            });
        }
        Self::checked(items, u64::try_from(DEEP_ITEMS).unwrap())
    }

    /// Two items totaling the exact accepted product/model byte boundary.
    #[must_use]
    pub fn exact_budget_boundary() -> Self {
        Self::checked(
            vec![
                HistoryItem {
                    role: HistoryRole::User,
                    text: padded_ascii("budget-user|", 131_072, b'u'),
                },
                HistoryItem {
                    role: HistoryRole::Assistant,
                    text: padded_ascii("budget-assistant|", 131_072, b'a'),
                },
            ],
            4,
        )
    }

    /// Two items totaling one byte above the product ceiling.
    #[must_use]
    pub fn product_overflow() -> Self {
        Self::checked(
            vec![
                HistoryItem {
                    role: HistoryRole::User,
                    text: padded_ascii("overflow-user|", 131_072, b'u'),
                },
                HistoryItem {
                    role: HistoryRole::Assistant,
                    text: padded_ascii("overflow-assistant|", 131_073, b'a'),
                },
            ],
            5,
        )
    }

    fn checked(items: Vec<HistoryItem>, expected_pages: u64) -> Self {
        assert!(!items.is_empty());
        assert_eq!(items.len() % 2, 0);
        for (index, item) in items.iter().enumerate() {
            let expected = if index % 2 == 0 {
                HistoryRole::User
            } else {
                HistoryRole::Assistant
            };
            assert_eq!(item.role, expected);
            assert_json_literal_safe(&item.text);
        }
        Self {
            items,
            expected_pages,
        }
    }

    pub fn install(
        &self,
        fixture: &mut Fixture,
        thread: SyndicThreadId,
        pending_text: &str,
    ) -> InstalledHistory {
        for turn in self.items.chunks_exact(2) {
            let submitted = fixture.submit_text_on(thread, turn[0].text());
            fixture.complete_with_assistant_on(thread, submitted, turn[1].text());
        }
        fixture.submit_text_on(thread, pending_text);
        let completed_prefix = fixture.native_source(thread).binding().represented_prefix();
        let selected_path = fixture.selected_path(thread);
        fixture.retire_current_binding(thread);
        InstalledHistory {
            thread,
            selected_path,
            completed_prefix,
        }
    }

    #[must_use]
    pub fn prepare_exact_projection(
        &self,
        fixture: &Fixture,
        installed: InstalledHistory,
        model_context_window_tokens: u64,
    ) -> RecoveryProjection {
        let assembly = fixture
            .storage
            .prepare_recovery_projection(
                &fixture.store,
                RecoveryProjectionRequest::for_pending_selected_turn_parent(
                    installed.thread,
                    installed.selected_path,
                    Some(model_context_window_tokens),
                ),
            )
            .unwrap();
        let RecoveryAssembly::Ready(projection) = assembly else {
            panic!("phase44 nonempty history unexpectedly prepared an empty projection")
        };
        assert_eq!(u64::from(projection.item_count().get()), self.item_count());
        assert_eq!(projection.utf8_bytes().get(), self.utf8_bytes());
        assert_eq!(projection.sequence_digest(), self.sequence_digest());
        projection
    }

    #[must_use]
    pub fn item_count(&self) -> u64 {
        u64::try_from(self.items.len()).unwrap()
    }

    #[must_use]
    pub fn utf8_bytes(&self) -> u64 {
        self.items
            .iter()
            .map(|item| u64::try_from(item.text.len()).unwrap())
            .sum()
    }

    #[must_use]
    pub const fn expected_pages(&self) -> u64 {
        self.expected_pages
    }

    #[must_use]
    pub fn sequence_digest(&self) -> RecoveryItemSequenceDigest {
        let mut digest = RecoveryItemSequenceAccumulator::new(self.item_count(), self.utf8_bytes());
        for (index, item) in self.items.iter().enumerate() {
            digest
                .begin_item(
                    u64::try_from(index + 1).unwrap(),
                    item.role.sequence_role(),
                    u64::try_from(item.text.len()).unwrap(),
                )
                .unwrap();
            digest.update_text(item.text.as_bytes()).unwrap();
            digest.finish_item().unwrap();
        }
        digest.finish().unwrap()
    }
}

impl HistoryItem {
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

impl HistoryRole {
    const fn sequence_role(self) -> RecoveryItemSequenceRole {
        match self {
            Self::User => RecoveryItemSequenceRole::UserInputText,
            Self::Assistant => RecoveryItemSequenceRole::AssistantOutputText,
        }
    }
}

fn split_scalar_text(prefix: &str) -> String {
    let mut text = padded_ascii(prefix, FRAGMENTED_LONG_ITEM_BYTES - "é".len(), b'x');
    assert_eq!(text.len(), 65_535);
    text.push('é');
    assert_eq!(text.len(), FRAGMENTED_LONG_ITEM_BYTES);
    text
}

const fn deep_item_bytes(index: usize) -> usize {
    if index < DEEP_LONG_ITEM_COUNT {
        DEEP_ITEM_BASE_BYTES + 1
    } else {
        DEEP_ITEM_BASE_BYTES
    }
}

fn padded_ascii(prefix: &str, total_bytes: usize, fill: u8) -> String {
    assert!(fill.is_ascii_alphanumeric());
    assert!(prefix.len() <= total_bytes);
    assert_json_literal_safe(prefix);
    let mut text = String::with_capacity(total_bytes);
    text.push_str(prefix);
    text.extend(std::iter::repeat_n(
        char::from(fill),
        total_bytes - prefix.len(),
    ));
    assert_eq!(text.len(), total_bytes);
    text
}

fn assert_json_literal_safe(value: &str) {
    assert!(
        value
            .chars()
            .all(|character| !character.is_control() && !matches!(character, '"' | '\\')),
        "phase44 exact-wire values deliberately require no JSON escaping"
    );
}

enum ExpectedSegment<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

pub struct ExpectedInjection<'a> {
    segments: Vec<ExpectedSegment<'a>>,
    segment: usize,
    offset: usize,
    logical_bytes: u64,
}

impl<'a> ExpectedInjection<'a> {
    #[must_use]
    pub fn new(request_id: u64, target_thread: &'a str, history: &'a HistorySpec) -> Self {
        assert_json_literal_safe(target_thread);
        let mut segments = Vec::with_capacity(history.items.len() * 3 + 6);
        segments.push(ExpectedSegment::Borrowed(br#"{"jsonrpc":"2.0","id":"#));
        segments.push(ExpectedSegment::Owned(request_id.to_string().into_bytes()));
        segments.push(ExpectedSegment::Borrowed(
            br#","method":"thread/inject_items","params":{"threadId":""#,
        ));
        segments.push(ExpectedSegment::Borrowed(target_thread.as_bytes()));
        segments.push(ExpectedSegment::Borrowed(br#"","items":["#));
        for (index, item) in history.items.iter().enumerate() {
            if index != 0 {
                segments.push(ExpectedSegment::Borrowed(b","));
            }
            segments.push(ExpectedSegment::Borrowed(match item.role {
                HistoryRole::User => {
                    br#"{"type":"message","role":"user","content":[{"type":"input_text","text":""#
                }
                HistoryRole::Assistant => {
                    br#"{"type":"message","role":"assistant","content":[{"type":"output_text","text":""#
                }
            }));
            segments.push(ExpectedSegment::Borrowed(item.text.as_bytes()));
            segments.push(ExpectedSegment::Borrowed(br#""}]}"#));
        }
        segments.push(ExpectedSegment::Borrowed(br#"]}}"#));
        let logical_bytes = segments
            .iter()
            .map(|segment| u64::try_from(segment.bytes().len()).unwrap())
            .sum();
        Self {
            segments,
            segment: 0,
            offset: 0,
            logical_bytes,
        }
    }

    #[must_use]
    pub const fn logical_bytes(&self) -> u64 {
        self.logical_bytes
    }
}

impl ExpectedSegment<'_> {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}

impl Iterator for ExpectedInjection<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let current = self.segments.get(self.segment)?.bytes();
            if let Some(byte) = current.get(self.offset).copied() {
                self.offset += 1;
                return Some(byte);
            }
            self.segment += 1;
            self.offset = 0;
        }
    }
}
