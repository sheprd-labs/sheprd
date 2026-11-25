import Foundation
import Testing
@testable import SheprdRemoteCore

struct NewProjectTests {
    @Test func encodesAnEmptyProjectAndACloneForTheComputer() throws {
        for source: String? in [nil, "org/forecast"] {
            let request = try #require(NewProjectRequest(root: "/work", name: " forecast ", clone: source))
            let encoded = try JSONEncoder().encode(request)
            var expected = ["root": "/work", "name": "forecast"]
            expected["clone"] = source
            #expect(try JSONDecoder().decode([String: String].self, from: encoded) == expected)
        }
    }

    @Test func requiresARootAndASingleVisibleFolder() {
        for name in ["", "../escape", ".hidden", "-option", "two/folders", "two\\folders", "disk:name"] {
            #expect(NewProjectRequest(root: "/work", name: name, clone: nil) == nil)
        }
        #expect(NewProjectRequest(root: "", name: "forecast", clone: nil) == nil)
        #expect(NewProjectRequest(root: "/work", name: "forecast", clone: "   ") == nil)
    }
}
