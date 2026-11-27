import SheprdRemoteCore
import XCTest
@testable import SheprdRemote

final class ComputerNamesTests: XCTestCase {
    func testSessionHostNames() {
        let studio = DemoData.studio
        var laptop = DemoData.laptop
        laptop.name = studio.name
        let computers = [studio, laptop]

        XCTAssertEqual([studio].displayName(forDevice: studio.device), studio.name)
        for host in computers {
            XCTAssertEqual(computers.displayName(forDevice: host.device),
                           "\(host.name) (\(host.fingerprint.hex.prefix(8)))")
        }
        XCTAssertNil(computers.displayName(forDevice: "unknown"))
    }
}
