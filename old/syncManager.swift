//
//  syncManager.swift
//  screenhistory
//
//  Created by quigon on 26/11/2023.
//

import Foundation


class SyncManager {
    var lastSync: TimeInterval?

    func syncIfNeeded() {
        let lastSyncDate = Date(timeIntervalSince1970: lastSync ?? 0)
        let components = Calendar.current.dateComponents([.day], from: lastSyncDate, to: Date())
        if let day = components.day, day >= 7 {
            sync()
        }
    }

    private func sync() {
        guard let usage = fetchScreenTimeData() else { return }
        print(usage[0])
    }

}
