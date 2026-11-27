import SheprdRemoteCore

extension Array where Element == Computer {
    func displayName(forDevice device: String) -> String? {
        first { $0.device == device }.map { displayName(of: $0) }
    }

    func displayName(of computer: Computer) -> String {
        let twins = filter { $0.name == computer.name }.count
        guard twins > 1 else { return computer.name }
        return "\(computer.name) (\(computer.fingerprint.hex.prefix(8)))"
    }
}
