/// How many lines a list row's title and its detail may each take: two
/// settings, five lines at most, so a row grows to show more of a title or
/// reply but never pushes the list into a wall of text.
public enum RowLines {
    /// The `UserDefaults` key the detail line's setting is kept under.
    public static let key = "rowLines"
    /// The detail's lines until the person picks: one, as a mail list shows.
    public static let detailDefault = 1
    /// The `UserDefaults` key the title's setting is kept under.
    public static let titleKey = "rowTitleLines"
    /// The title's lines until the person picks: two, so a long title,
    /// often a chat's first prompt, reads past its first words.
    public static let titleDefault = 2
    public static let choices = 1...5

    /// `stored` within the choices: an older or edited value cannot make a
    /// row blank or unbounded.
    public static func clamped(_ stored: Int) -> Int {
        min(max(stored, choices.lowerBound), choices.upperBound)
    }
}
