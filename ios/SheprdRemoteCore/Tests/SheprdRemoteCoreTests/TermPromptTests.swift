import Foundation
import Testing
@testable import SheprdRemoteCore

/// A terminal agent's question as its computer reads it off the screen.
struct TermPromptTests {
    @Test func decodesAQuestionWithItsChoices() throws {
        let answer = try JSONDecoder().decode(TermPrompt.Answer.self, from: WireTests.fixture("term_prompt"))
        let prompt = try #require(answer.prompt)
        #expect(prompt.question == ["Deploy", "Which environment?"])
        #expect(prompt.selected == 0)
        #expect(prompt.choices.map(\.label) == ["Staging", "Production", "Type something."])
        #expect(prompt.choices[0].detail == "The one the team shares.")
        #expect(prompt.choices.map(\.takesWords) == [false, false, true])
    }

    @Test func decodesAPaneWithNoQuestion() throws {
        let answer = try JSONDecoder().decode(TermPrompt.Answer.self, from: WireTests.fixture("term_prompt_none"))
        #expect(answer.prompt == nil)
    }
}
