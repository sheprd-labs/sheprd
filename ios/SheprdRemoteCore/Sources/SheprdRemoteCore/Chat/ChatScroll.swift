import CoreGraphics

/// Where a chat's scroll view rests against the end of its content.
public enum ChatScroll {
    /// Whether the offset lies past the furthest the content scrolls, as
    /// when the content shrank under a resting offset. A screen there shows
    /// nothing until a drag, so the chat snaps back to the latest.
    public static func pastEnd(
        offset: CGFloat,
        contentHeight: CGFloat,
        containerHeight: CGFloat,
        insetTop: CGFloat,
        insetBottom: CGFloat
    ) -> Bool {
        let end = contentHeight + insetBottom - containerHeight
        // Content shorter than the screen rests at its top.
        return offset > max(end, -insetTop) + 1
    }
}
