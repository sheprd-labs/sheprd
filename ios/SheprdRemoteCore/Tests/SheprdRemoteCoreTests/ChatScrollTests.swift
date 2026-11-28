import CoreGraphics
@testable import SheprdRemoteCore
import Testing

struct ChatScrollTests {
    private func pastEnd(_ offset: CGFloat, content: CGFloat = 5000) -> Bool {
        ChatScroll.pastEnd(offset: offset, contentHeight: content, containerHeight: 800, insetTop: 120, insetBottom: 200)
    }

    @Test func atTheLatestIsNotPastTheEnd() {
        #expect(!pastEnd(4400))
        #expect(!pastEnd(4401))
    }

    @Test func scrolledUpIsNotPastTheEnd() {
        #expect(!pastEnd(1200))
        #expect(!pastEnd(-120))
    }

    @Test func contentShrunkUnderTheOffsetIsPastTheEnd() {
        #expect(pastEnd(4400, content: 3000))
    }

    @Test func shortContentRestsAtItsTop() {
        #expect(!pastEnd(-120, content: 300))
        #expect(pastEnd(40, content: 300))
    }
}
