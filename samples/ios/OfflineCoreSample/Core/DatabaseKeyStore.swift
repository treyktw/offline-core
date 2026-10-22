import Foundation
import Security

/// Loads or creates the 32-byte database key in the Keychain. The Rust core
/// never generates or stores key material itself.
struct DatabaseKeyStore: Sendable {
    private let service = "com.example.offlinecore.database"
    private let account = "encrypted-database-key-v1"

    func loadOrCreate() throws -> Data {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecSuccess, let data = result as? Data, data.count == 32 {
            return data
        }
        guard status == errSecItemNotFound else {
            throw DatabaseKeyError.keychain(status)
        }

        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else {
            throw DatabaseKeyError.randomGeneration
        }
        let data = Data(bytes)
        let insert: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
            kSecValueData as String: data,
        ]
        let insertStatus = SecItemAdd(insert as CFDictionary, nil)
        guard insertStatus == errSecSuccess else {
            throw DatabaseKeyError.keychain(insertStatus)
        }
        return data
    }
}

enum DatabaseKeyError: LocalizedError {
    case keychain(OSStatus)
    case randomGeneration

    var errorDescription: String? {
        switch self {
        case let .keychain(status): "Keychain operation failed (\(status))."
        case .randomGeneration: "Could not generate the local database key."
        }
    }
}
