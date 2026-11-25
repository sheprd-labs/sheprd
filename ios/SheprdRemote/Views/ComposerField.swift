import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// The composer's text field: a system text view that also takes pictures,
/// as Messages' does. A picture pasted from the edit menu, the keyboard's
/// suggestion, ⌘V, or dropped on it goes to `pictures` rather than into the
/// words. Grows from one line to `maxLines`, then scrolls; follows Dynamic
/// Type.
struct ComposerField: UIViewRepresentable {
    let placeholder: String
    @Binding var text: String
    @Binding var focused: Bool
    var maxLines = 6
    let pictures: ([UIImage]) -> Void

    func makeUIView(context: Context) -> PicturesTextView {
        let view = PicturesTextView()
        view.delegate = context.coordinator
        view.font = .preferredFont(forTextStyle: .body)
        view.adjustsFontForContentSizeCategory = true
        view.backgroundColor = .clear
        view.textContainerInset = .zero
        view.textContainer.lineFragmentPadding = 0
        // Keep native caret scrolling enabled even while the field is growing.
        // A speculative layout width must never change the editor’s scroll state.
        view.alwaysBounceVertical = false
        view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        view.pasteConfiguration = UIPasteConfiguration(acceptableTypeIdentifiers: [
            UTType.image.identifier, UTType.plainText.identifier, UTType.url.identifier,
        ])
        view.placeholder.text = placeholder
        view.accessibilityLabel = placeholder
        return view
    }

    func updateUIView(_ view: PicturesTextView, context: Context) {
        context.coordinator.parent = self
        view.pictures = pictures
        if view.text != text {
            view.text = text
            view.invalidateIntrinsicContentSize()
        }
        view.placeholder.text = placeholder
        view.placeholder.isHidden = !text.isEmpty
        // Focus follows the binding both ways, deferred so a responder
        // change never happens inside SwiftUI's update.
        if focused != view.isFirstResponder {
            Task { @MainActor in
                if focused { view.becomeFirstResponder() } else { view.resignFirstResponder() }
            }
        }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView view: PicturesTextView, context: Context) -> CGSize? {
        let width = proposal.width ?? 280
        let line = view.font?.lineHeight ?? 22
        let fitting = view.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude)).height
        let height = min(max(fitting, line), line * CGFloat(maxLines))
        return CGSize(width: width, height: ceil(height))
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    @MainActor
    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: ComposerField

        init(_ parent: ComposerField) { self.parent = parent }

        func textViewDidChange(_ view: UITextView) {
            parent.text = view.text
            view.invalidateIntrinsicContentSize()
        }

        func textViewDidBeginEditing(_ view: UITextView) {
            if !parent.focused { parent.focused = true }
        }

        func textViewDidEndEditing(_ view: UITextView) {
            if parent.focused { parent.focused = false }
        }
    }
}

/// A text view whose paste and drop take pictures as well as words.
final class PicturesTextView: UITextView {
    var pictures: (([UIImage]) -> Void)?
    let placeholder = UILabel()

    override init(frame: CGRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        placeholder.font = .preferredFont(forTextStyle: .body)
        placeholder.adjustsFontForContentSizeCategory = true
        placeholder.textColor = .placeholderText
        placeholder.isAccessibilityElement = false
        placeholder.translatesAutoresizingMaskIntoConstraints = false
        addSubview(placeholder)
        NSLayoutConstraint.activate([
            placeholder.leadingAnchor.constraint(equalTo: leadingAnchor),
            placeholder.topAnchor.constraint(equalTo: topAnchor),
            placeholder.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { nil }

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if action == #selector(paste(_:)), UIPasteboard.general.hasImages { return true }
        return super.canPerformAction(action, withSender: sender)
    }

    /// The edit menu's Paste and ⌘V: a picture on the clipboard goes to the
    /// pictures, its words, if any, into the text.
    override func paste(_ sender: Any?) {
        let board = UIPasteboard.general
        guard board.hasImages, let images = board.images, !images.isEmpty else {
            super.paste(sender)
            return
        }
        pictures?(images)
        if board.hasStrings, let words = board.string, !words.isEmpty {
            insertText(words)
        }
    }

    /// What the keyboard's paste suggestion and a drop hand over: pictures
    /// to the pictures, the rest, such as words, into the text.
    override func paste(itemProviders: [NSItemProvider]) {
        let images = itemProviders.filter { $0.canLoadObject(ofClass: UIImage.self) }
        let rest = itemProviders.filter { !$0.canLoadObject(ofClass: UIImage.self) }
        if !rest.isEmpty { super.paste(itemProviders: rest) }
        for provider in images {
            provider.loadObject(ofClass: UIImage.self) { [weak self] object, _ in
                guard let image = object as? UIImage else { return }
                Task { @MainActor in self?.pictures?([image]) }
            }
        }
    }
}
