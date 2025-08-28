//
//  fetchData.swift
//  screenhistory
//
//  Created by quigon on 21/07/2023.
//

import Foundation
import PerfectSQLite

let query = """
                 SELECT
                     ZOBJECT.ZVALUESTRING AS "app",
                     (ZOBJECT.ZENDDATE - ZOBJECT.ZSTARTDATE) AS "usage",
                     (ZOBJECT.ZSTARTDATE + 978307200) as "start_time",
                     (ZOBJECT.ZENDDATE + 978307200) as "end_time",
                     (ZOBJECT.ZCREATIONDATE + 978307200) as "created_at",
                     ZOBJECT.ZSECONDSFROMGMT AS "tz",
                     ZSOURCE.ZDEVICEID AS "device_id",
                     ZMODEL AS "device_model"
                 FROM
                     ZOBJECT
                     LEFT JOIN
                     ZSTRUCTUREDMETADATA
                     ON ZOBJECT.ZSTRUCTUREDMETADATA = ZSTRUCTUREDMETADATA.Z_PK
                     LEFT JOIN
                     ZSOURCE
                     ON ZOBJECT.ZSOURCE = ZSOURCE.Z_PK
                     LEFT JOIN
                     ZSYNCPEER
                     ON ZSOURCE.ZDEVICEID = ZSYNCPEER.ZDEVICEID
                 WHERE
                     ZSTREAMNAME = '/app/usage'
                 ORDER BY
                     ZSTARTDATE DESC
  """

let screenTimeDB = NSString(string: "~/Library/Application Support/Knowledge/knowledgeC.db").expandingTildeInPath

let createQuery = """
    CREATE TABLE Usage (
        appName TEXT NOT NULL,
        amount REAL NOT NULL,            -- TimeInterval is a typealias for Double
        startTime TEXT NOT NULL,         -- Dates can be stored as TEXT in ISO8601 format
        endTime TEXT NOT NULL,
        createdAt TEXT NOT NULL,
        timezoneOffset REAL NOT NULL,    -- TimeInterval for timezone offset
        deviceId TEXT NOT NULL,          -- UUID as TEXT
        deviceModel TEXT NOT NULL
    );
  """

let latestQuery = """
    SELECT
        appName,
        amount,
        startTime,
        endTime,
        createdAt,
        timezoneOffset,
        deviceId,
        deviceModel
    FROM
        Usage
    ORDER BY
        endTime DESC
    LIMIT 1;
  """

struct Usage {
  let appName: String
  let amount: TimeInterval
  let startTime: Date
  let endTime: Date
  let createdAt: Date
  let timezoneOffset: TimeInterval
  let deviceId: UUID?
  let deviceModel: String
}


func fetchScreenTimeData() -> [Usage]? {
  // Fetch the latest usage data from the apple screen time database

  var usages: [Usage] = []
  do {
    let sqlite = try SQLite(screenTimeDB)
    defer {
      sqlite.close()  // This makes sure we close our connection.
    }

    try sqlite.forEachRow(
      statement: query,
      doBindings: {

        (statement: SQLiteStmt) -> Void in
      }
    ) { (statement: SQLiteStmt, i: Int) -> Void in
      let appName = statement.columnText(position: 0)

      let amount = TimeInterval(statement.columnInt(position: 1))

      let startTime = Date.init(timeIntervalSince1970: statement.columnDouble(position: 2))
      let endTime = Date.init(timeIntervalSince1970: statement.columnDouble(position: 3))

      let createdAt = Date.init(timeIntervalSince1970: statement.columnDouble(position: 4))

      let timezoneOffset = TimeInterval(statement.columnInt(position: 5))

      //device uuid can be null
      let deviceId = UUID(uuidString: statement.columnText(position: 6))

      let deviceModel = statement.columnText(position: 7)

      usages.append(Usage(
        appName: appName, amount: amount, startTime: startTime, endTime: endTime,
        createdAt: createdAt, timezoneOffset: timezoneOffset, deviceId: deviceId,
        deviceModel: deviceModel))
    }
  } catch (let e) {
      print("Error fetching screentime db: \(e)")
      return nil
  }
  return usages
}

func latestUsage(screenHistoryPath: String) -> Usage? {

  var usage: Usage?
  do {
    let sqlite = try SQLite(screenHistoryPath)
    defer { sqlite.close() }

    // try and get the latest usage
    try sqlite.execute(statement: latestQuery) { (statement: SQLiteStmt) -> Void in
      let appName = statement.columnText(position: 0)

      let amount = TimeInterval(statement.columnInt(position: 1))

      let startTime = Date.init(timeIntervalSince1970: statement.columnDouble(position: 2))
      let endTime = Date.init(timeIntervalSince1970: statement.columnDouble(position: 3))

      let createdAt = Date.init(timeIntervalSince1970: statement.columnDouble(position: 4))

      let timezoneOffset = TimeInterval(statement.columnInt(position: 5))

      let deviceId = UUID(uuidString: statement.columnText(position: 6))!
      let deviceModel = statement.columnText(position: 7)

      usage = Usage(
        appName: appName, amount: amount, startTime: startTime, endTime: endTime,
        createdAt: createdAt, timezoneOffset: timezoneOffset, deviceId: deviceId,
        deviceModel: deviceModel)
    }
  } catch (let e) {
    print("Error fetching latest usage: \(e)")
  }
  return usage
}
