import Foundation
@testable import RelayClient
import Testing

struct RelayAddressTests {
    @Test func endpointsReadAsTheRelayCrateReadsThem() throws {
        let vector = try Vectors.load("websocket", as: Vectors.WebSocket.self)
        let route = try Vectors.hex(vector.listener.route)
        #expect(vector.endpoints.count == 5)
        for expected in vector.endpoints {
            let address = try RelayAddress(expected.endpoint)
            #expect(address.proofName == expected.proof_name, "\(expected.endpoint)")
            #expect(address.info.absoluteString == expected.info, "\(expected.endpoint)")
            #expect(address.listen.absoluteString == expected.listen, "\(expected.endpoint)")
            #expect(address.dial(route: route).absoluteString == expected.dial, "\(expected.endpoint)")
        }
    }

    @Test func acceptAndDescriptionKeepThePrefix() throws {
        let address = try RelayAddress("relay.example.org:8443/acme/")
        #expect(address.description == "https://relay.example.org:8443/acme")
        #expect(address.accept(session: "AAAAAAAAAAAAAAAAAAAAAA").absoluteString
            == "wss://relay.example.org:8443/acme/v1/accept/AAAAAAAAAAAAAAAAAAAAAA")
        #expect(try RelayAddress("relay.example.org").description == "https://relay.example.org")
        let data = try JSONEncoder().encode(address)
        #expect(try JSONDecoder().decode(RelayAddress.self, from: data) == address)
    }

    @Test func plainHTTPIsOnlyForLocalHosts() throws {
        for local in [
            "http://localhost:8080", "ws://relay.local", "http://10.0.0.5", "http://172.20.1.1",
            "http://192.168.1.20:7443", "http://169.254.0.1", "http://[::1]:8080", "http://[fd00::1]",
            "http://[fe80::1]", "http://[::ffff:192.168.1.1]",
        ] {
            #expect(throws: Never.self, "\(local)") { try RelayAddress(local) }
        }
        for remote in ["http://relay.example.org", "ws://8.8.8.8", "http://172.32.0.1", "http://[2001:db8::1]"] {
            #expect(throws: RelayAddress.Error.insecure, "\(remote)") { try RelayAddress(remote) }
        }
        let ipv6 = try RelayAddress("http://[::1]:8080/acme")
        #expect(ipv6.host == "::1")
        #expect(ipv6.proofName == "[::1]:8080/acme")
        #expect(ipv6.dial(route: Data(count: 32)).absoluteString
            == "ws://[::1]:8080/acme/v1/dial/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
    }

    @Test func otherFormsAreRefused() {
        #expect(throws: RelayAddress.Error.scheme("ftp")) { try RelayAddress("ftp://relay.example.org") }
        #expect(throws: RelayAddress.Error.credentials) { try RelayAddress("https://user@relay.example.org") }
        #expect(throws: RelayAddress.Error.extra) { try RelayAddress("https://relay.example.org/?a=1") }
        #expect(throws: RelayAddress.Error.extra) { try RelayAddress("https://relay.example.org/#top") }
        #expect(throws: RelayAddress.Error.noHost) { try RelayAddress("https://") }
        #expect(throws: RelayAddress.Error.invalid) { try RelayAddress("relay.example.org:99999") }
    }

    @Test func theDefaultPortIsDroppedAndOthersKept() throws {
        #expect(try RelayAddress("wss://relay.example.org:443").proofName == "relay.example.org")
        #expect(try RelayAddress("http://localhost:80").proofName == "localhost")
        #expect(try RelayAddress("http://localhost:443").proofName == "localhost:443")
        #expect(try RelayAddress(" Relay.Example.org ").proofName == "relay.example.org")
    }
}
