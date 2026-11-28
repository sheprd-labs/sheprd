@testable import SheprdRemoteCore
import Testing

/// The rows' line settings stay between one and five lines.
struct RowLinesTests {
    @Test func keepsAChoice() {
        for lines in RowLines.choices { #expect(RowLines.clamped(lines) == lines) }
    }

    @Test func boundsAnythingElse() {
        #expect(RowLines.clamped(0) == 1)
        #expect(RowLines.clamped(-4) == 1)
        #expect(RowLines.clamped(9) == 5)
    }

    @Test func defaultsAreChoices() {
        #expect(RowLines.choices.contains(RowLines.detailDefault))
        #expect(RowLines.choices.contains(RowLines.titleDefault))
        #expect(RowLines.titleDefault == 2)
    }
}
