import SheprdRelay
import SheprdRemoteCore

extension Fleet {
    func bannedRelay(for device: String) async -> RelayEndpoint? {
        guard relayOrigin(device) != nil,
              let record = machines.first(where: { $0.id == device })?.computer.relay
        else { return nil }
        return await withTaskGroup(of: RelayEndpoint?.self) { group in
            for relay in record.candidates(saved: savedRelays) {
                group.addTask { await RelayReach.probe(relay) == .banned ? relay : nil }
            }
            for await relay in group {
                if let relay {
                    group.cancelAll()
                    return relay
                }
            }
            return nil
        }
    }
}
