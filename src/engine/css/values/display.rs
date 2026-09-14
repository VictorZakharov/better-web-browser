//! CSS display roles shared by computed styles and box-tree construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    None,
    Contents,
    Block,
    FlowRoot,
    Inline,
    InlineBlock,
    InlineFlex,
    Flex,
    Grid,
    Table,
    InlineTable,
    TableRow,
    TableCell,
    TableCaption,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableColumn,
    TableColumnGroup,
}

impl Display {
    pub(crate) const fn is_table(self) -> bool {
        matches!(self, Self::Table | Self::InlineTable)
    }

    pub(crate) const fn css_keyword(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Contents => "contents",
            Self::Block => "block",
            Self::FlowRoot => "flow-root",
            Self::Inline => "inline",
            Self::InlineBlock => "inline-block",
            Self::InlineFlex => "inline-flex",
            Self::Flex => "flex",
            Self::Grid => "grid",
            Self::Table => "table",
            Self::InlineTable => "inline-table",
            Self::TableRow => "table-row",
            Self::TableCell => "table-cell",
            Self::TableCaption => "table-caption",
            Self::TableRowGroup => "table-row-group",
            Self::TableHeaderGroup => "table-header-group",
            Self::TableFooterGroup => "table-footer-group",
            Self::TableColumn => "table-column",
            Self::TableColumnGroup => "table-column-group",
        }
    }
}
