import ApplicationServices
import CoreGraphics
import Foundation

/// Where synthetic input goes: the session's event stream, as a person's
/// mouse and keyboard would.
private let tap = CGEventTapLocation.cghidEventTap
/// A pause between events, so apps see them in order.
private let gap: useconds_t = 8_000

private func trusted() throws {
    try require(\.accessibility, refusal: .accessibility, ask: askForAccessibility)
}

private func number(_ value: Any?) -> Double? {
    (value as? NSNumber)?.doubleValue
}

func click(_ request: [String: Any]) throws -> [String: Any] {
    guard let x = number(request["x"]), let y = number(request["y"]) else {
        throw Refusal.invalid("click needs x and y")
    }
    try trusted()
    let scale = display().scale
    let point = CGPoint(x: x / scale, y: y / scale)
    let right = (request["button"] as? String) == "right"
    let count = min(3, max(1, (request["count"] as? Int) ?? 1))
    let (down, up, button): (CGEventType, CGEventType, CGMouseButton) = right
        ? (.rightMouseDown, .rightMouseUp, .right)
        : (.leftMouseDown, .leftMouseUp, .left)
    let source = CGEventSource(stateID: .hidSystemState)
    CGEvent(mouseEventSource: source, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: button)?
        .post(tap: tap)
    usleep(gap)
    for click in 1...count {
        for type in [down, up] {
            let event = CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: point, mouseButton: button)
            event?.setIntegerValueField(.mouseEventClickState, value: Int64(click))
            event?.post(tap: tap)
            usleep(gap)
        }
    }
    return [:]
}

/// Types text as if from a keyboard, whatever the layout: each event
/// carries its characters rather than a key.
func type(_ request: [String: Any]) throws -> [String: Any] {
    guard let text = request["text"] as? String else { throw Refusal.invalid("type needs text") }
    try trusted()
    let source = CGEventSource(stateID: .hidSystemState)
    for character in text {
        let units = Array(String(character).utf16)
        for down in [true, false] {
            let event = CGEvent(keyboardEventSource: source, virtualKey: 0, keyDown: down)
            event?.flags = []
            event?.keyboardSetUnicodeString(stringLength: units.count, unicodeString: units)
            event?.post(tap: tap)
        }
        usleep(gap)
    }
    return [:]
}

/// Presses one key, with modifiers held: `return`, `tab`, `a`, `cmd`+`q`.
func key(_ request: [String: Any]) throws -> [String: Any] {
    guard let name = (request["key"] as? String)?.lowercased(), let code = keyCodes[name] else {
        throw Refusal.invalid("unknown key")
    }
    var flags = CGEventFlags()
    for modifier in (request["modifiers"] as? [String]) ?? [] {
        switch modifier {
        case "cmd": flags.insert(.maskCommand)
        case "shift": flags.insert(.maskShift)
        case "option": flags.insert(.maskAlternate)
        case "control": flags.insert(.maskControl)
        case "fn": flags.insert(.maskSecondaryFn)
        default: throw Refusal.invalid("unknown modifier \(modifier)")
        }
    }
    try trusted()
    let source = CGEventSource(stateID: .hidSystemState)
    for down in [true, false] {
        let event = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down)
        event?.flags = flags
        event?.post(tap: tap)
        usleep(gap)
    }
    return [:]
}

/// Keys by name, at their places on an ANSI keyboard.
let keyCodes: [String: CGKeyCode] = [
    "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9,
    "b": 11, "q": 12, "w": 13, "e": 14, "r": 15, "y": 16, "t": 17, "1": 18, "2": 19,
    "3": 20, "4": 21, "6": 22, "5": 23, "=": 24, "9": 25, "7": 26, "-": 27, "8": 28,
    "0": 29, "]": 30, "o": 31, "u": 32, "[": 33, "i": 34, "p": 35, "return": 36,
    "l": 37, "j": 38, "'": 39, "k": 40, ";": 41, "\\": 42, ",": 43, "/": 44, "n": 45,
    "m": 46, ".": 47, "tab": 48, "space": 49, "`": 50, "delete": 51, "escape": 53,
    "f17": 64, "f18": 79, "f19": 80, "f20": 90, "f5": 96, "f6": 97, "f7": 98, "f3": 99,
    "f8": 100, "f9": 101, "f11": 103, "f13": 105, "f16": 106, "f14": 107, "f10": 109,
    "f12": 111, "f15": 113, "home": 115, "pageup": 116, "forwarddelete": 117, "f4": 118,
    "end": 119, "f2": 120, "pagedown": 121, "f1": 122, "left": 123, "right": 124,
    "down": 125, "up": 126,
]
