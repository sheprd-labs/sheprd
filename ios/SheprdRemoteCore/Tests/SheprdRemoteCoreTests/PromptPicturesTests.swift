import Testing
@testable import SheprdRemoteCore

/// Only pictures this app uploaded become picture marks.
struct PromptPicturesTests {
    @Test func anUploadedPictureBecomesAMark() {
        let (words, pictures) = PromptPictures.split("/state/remote/uploads/oLvM3S2h3bnDdk7l3dToDw.jpg Pasting a screenshot")
        #expect(words == "Pasting a screenshot")
        #expect(pictures == 1)
    }

    @Test func aTypedPathStaysAsTyped() {
        let text = "Compare /tmp/before.png with /tmp/after.png"
        let (words, pictures) = PromptPictures.split(text)
        #expect(words == text)
        #expect(pictures == 0)
    }

    @Test func aNameThatIsNotAnUploadIdStays() {
        #expect(!PromptPictures.isUpload("/home/me/uploads/holiday.jpg"))
        #expect(!PromptPictures.isUpload("/home/me/uploads/oLvM3S2h3bnDdk7l3dToDw.png"))
        #expect(PromptPictures.isUpload("/x/uploads/oLvM3S2h3bnDdk7l3dToDw.jpg"))
    }
}
