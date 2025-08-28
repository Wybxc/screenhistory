//
//  screenhistoryApp.swift
//  screenhistory
//
//  Created by quigon on 13/07/2023.
//

import LaunchAtLogin
import SwiftUI

enum SyncStatus {
  case unknown
  case fullDiskAccessMissing
  case ableToSync

  var description: String {
    switch self {
    case .unknown:
      return "Unknown ❓"
    case .fullDiskAccessMissing:
      return "Missing Full Disk Access 🚫"
    case .ableToSync:
      return "Able to sync ✅"
    }
  }
}

@main
struct screenhistoryApp: App {
  private var syncManager = SyncManager()
  private var syncStatus = SyncStatus.unknown
  private let dbName = ".screenhistory.sqlite"
  private let defaultDirectory = FileManager.default.homeDirectoryForCurrentUser
  @State private var showingCopyConfirmation = false

  var dbLocation: URL {
    return defaultDirectory.appendingPathComponent(dbName)
  }

  var lastSyncFormatted: String {
    guard let lastSync = syncManager.lastSync else { return "Never" }
    return Date(timeIntervalSince1970: lastSync).formatted()
  }

  init() {
    setupTimer()

    if !hasFullDiskAccess() {
      syncStatus = .fullDiskAccessMissing
      showAlert()
    } else {
      syncStatus = .ableToSync
    }
  }

  private func setupTimer() {
    self.syncManager.syncIfNeeded()
    Timer.scheduledTimer(withTimeInterval: 60 * 60 * 24, repeats: true) { _ in
      self.syncManager.syncIfNeeded()
    }
  }

  func showAlert() {
    let alert = NSAlert()
    alert.messageText = "Full Disk Access Required"
    alert.informativeText =
      "Our application requires Full Disk Access to function properly. Please grant this permission in System Preferences."
    alert.addButton(withTitle: "OK")
    alert.addButton(withTitle: "Dismiss")

    let response = alert.runModal()
    if response == .alertFirstButtonReturn {
      // User clicked "OK", open System Preferences
      if let url = URL(
        string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
      {
        NSWorkspace.shared.open(url)
      }
    }
  }

  func hasFullDiskAccess() -> Bool {
    let fileManager = FileManager.default
    let testFilePath = screenTimeDB

    guard fileManager.fileExists(atPath: testFilePath) else {
      return false
    }

    // Attempt to read a small part of the file
    if let file = FileHandle(forReadingAtPath: testFilePath) {
      defer { file.closeFile() }
      return ((try? file.read(upToCount: 10)) != nil)
    } else {
      return false
    }

  }

  func copyToClipboard(_ text: String) {
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    pasteboard.setString(text, forType: .string)
  }

  var body: some Scene {
    MenuBarExtra("Screen History menu bar", systemImage: "star") {
      Text(syncStatus.description)
        .foregroundColor(syncStatus == .fullDiskAccessMissing ? .red : .primary)
        .onTapGesture {
          if syncStatus == .fullDiskAccessMissing {
            showAlert()
          }
        }
      Text("Last sync: \(lastSyncFormatted)")
      VStack {
        VStack {
          Button("Show Database in Finder") {
            let files = [dbLocation.absoluteURL]
            NSWorkspace.shared.activateFileViewerSelecting(files)
          }

        }
      }
      LaunchAtLogin.Toggle("Launch at login 🦄")
      Divider()
      Button("Quit") {

        NSApplication.shared.terminate(nil)

      }.keyboardShortcut("q")
    }
  }

}
