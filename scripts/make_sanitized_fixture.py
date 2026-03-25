#!/usr/bin/env python3

import argparse
import secrets
import sqlite3
import string
from pathlib import Path


ALPHABET = string.ascii_lowercase + string.digits


def random_token(length: int = 8) -> str:
    return "".join(secrets.choice(ALPHABET) for _ in range(length))


def random_app() -> str:
    return f"app_{random_token()}"


def random_device_id() -> str:
    return f"dev_{random_token()}"


def random_device_model() -> str:
    return f"model_{random_token()}"


def shift_time(value: float, min_start: float, base: float) -> float:
    return (value - min_start) + base


def create_schema(conn: sqlite3.Connection) -> None:
    conn.executescript(
        """
        CREATE TABLE ZOBJECT (
            Z_PK INTEGER PRIMARY KEY,
            ZSTREAMNAME TEXT,
            ZVALUESTRING TEXT,
            ZSTARTDATE REAL,
            ZENDDATE REAL,
            ZCREATIONDATE REAL,
            ZSECONDSFROMGMT INTEGER,
            ZSTRUCTUREDMETADATA INTEGER,
            ZSOURCE INTEGER
        );

        CREATE TABLE ZSTRUCTUREDMETADATA (
            Z_PK INTEGER PRIMARY KEY
        );

        CREATE TABLE ZSOURCE (
            Z_PK INTEGER PRIMARY KEY,
            ZDEVICEID TEXT,
            ZSOURCEID TEXT
        );

        CREATE TABLE ZSYNCPEER (
            Z_PK INTEGER PRIMARY KEY,
            ZDEVICEID TEXT,
            ZCLOUDID TEXT,
            ZMODEL TEXT,
            ZRAPPORTID TEXT
        );
        """
    )


def load_rows(src_conn: sqlite3.Connection, max_rows: int):
    query = """
        SELECT
            ZOBJECT.Z_PK AS event_id,
            ZOBJECT.ZVALUESTRING AS app_name,
            ZOBJECT.ZSTARTDATE AS start_date,
            ZOBJECT.ZENDDATE AS end_date,
            ZOBJECT.ZCREATIONDATE AS created_date,
            ZOBJECT.ZSECONDSFROMGMT AS tz_offset,
            COALESCE(ZSOURCE.ZDEVICEID, ZSYNCPEER.ZDEVICEID) AS device_id,
            COALESCE(ZSYNCPEER.ZMODEL, ZSYNCPEER.ZRAPPORTID, ZSOURCE.ZSOURCEID) AS device_model
        FROM ZOBJECT
        LEFT JOIN ZSTRUCTUREDMETADATA ON ZOBJECT.ZSTRUCTUREDMETADATA = ZSTRUCTUREDMETADATA.Z_PK
        LEFT JOIN ZSOURCE ON ZOBJECT.ZSOURCE = ZSOURCE.Z_PK
        LEFT JOIN ZSYNCPEER ON (
            ZSOURCE.ZDEVICEID = ZSYNCPEER.ZDEVICEID OR
            ZSOURCE.ZSOURCEID = ZSYNCPEER.ZCLOUDID
        )
        WHERE ZOBJECT.ZSTREAMNAME = '/app/usage'
        ORDER BY ZOBJECT.ZSTARTDATE ASC
        LIMIT ?
    """
    cur = src_conn.execute(query, (max_rows,))
    return cur.fetchall()


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Create a sanitized knowledgeC.db fixture for tests"
    )
    parser.add_argument(
        "--source",
        default="knowledgeC.db",
        help="Source knowledgeC.db path (default: ./knowledgeC.db)",
    )
    parser.add_argument(
        "--out",
        default="tests/fixtures/knowledgeC.sanitized.db",
        help="Output fixture DB path",
    )
    parser.add_argument(
        "--max-rows",
        type=int,
        default=1500,
        help="Maximum number of /app/usage rows",
    )

    args = parser.parse_args()

    source = Path(args.source)
    out = Path(args.out)

    if not source.exists():
        raise SystemExit(f"Source DB not found: {source}")

    out.parent.mkdir(parents=True, exist_ok=True)
    if out.exists():
        out.unlink()

    src_conn = sqlite3.connect(f"file:{source}?mode=ro", uri=True)
    out_conn = sqlite3.connect(out)

    try:
        create_schema(out_conn)
        rows = load_rows(src_conn, args.max_rows)
        if not rows:
            print("No /app/usage rows found in source DB.")
            return 0

        min_start = min((r[2] for r in rows if r[2] is not None), default=0.0)
        base_start = 7.0 * 24.0 * 3600.0

        app_map = {}
        device_id_map = {}
        device_model_map = {}
        source_map = {}
        syncpeer_map = {}

        with out_conn:
            for idx, row in enumerate(rows, start=1):
                _event_id, app_name, start_date, end_date, created_date, tz_offset, device_id, device_model = row

                raw_app_name = app_name or "unknown"
                if raw_app_name not in app_map:
                    app_map[raw_app_name] = random_app()
                app_name = app_map[raw_app_name]

                start_date = shift_time(float(start_date or 0.0), min_start, base_start)
                end_date = shift_time(float(end_date or start_date), min_start, base_start)
                created_date = shift_time(float(created_date or start_date), min_start, base_start)
                tz_offset = int(tz_offset or 0)

                sanitized_device_id = None
                if device_id:
                    if device_id not in device_id_map:
                        device_id_map[device_id] = random_device_id()
                    sanitized_device_id = device_id_map[device_id]

                raw_device_model = device_model or "model"
                if raw_device_model not in device_model_map:
                    device_model_map[raw_device_model] = random_device_model()
                sanitized_device_model = device_model_map[raw_device_model]

                zsource_pk = None
                if sanitized_device_id:
                    if sanitized_device_id not in source_map:
                        source_map[sanitized_device_id] = len(source_map) + 1
                        out_conn.execute(
                            "INSERT INTO ZSOURCE (Z_PK, ZDEVICEID, ZSOURCEID) VALUES (?, ?, ?)",
                            (
                                source_map[sanitized_device_id],
                                sanitized_device_id,
                                sanitized_device_id,
                            ),
                        )
                    zsource_pk = source_map[sanitized_device_id]

                    if sanitized_device_id not in syncpeer_map:
                        syncpeer_map[sanitized_device_id] = len(syncpeer_map) + 1
                        out_conn.execute(
                            """
                            INSERT INTO ZSYNCPEER (Z_PK, ZDEVICEID, ZCLOUDID, ZMODEL, ZRAPPORTID)
                            VALUES (?, ?, ?, ?, ?)
                            """,
                            (
                                syncpeer_map[sanitized_device_id],
                                sanitized_device_id,
                                sanitized_device_id,
                                sanitized_device_model,
                                sanitized_device_model,
                            ),
                        )

                out_conn.execute(
                    """
                    INSERT INTO ZOBJECT (
                        Z_PK,
                        ZSTREAMNAME,
                        ZVALUESTRING,
                        ZSTARTDATE,
                        ZENDDATE,
                        ZCREATIONDATE,
                        ZSECONDSFROMGMT,
                        ZSTRUCTUREDMETADATA,
                        ZSOURCE
                    ) VALUES (?, '/app/usage', ?, ?, ?, ?, ?, NULL, ?)
                    """,
                    (
                        idx,
                        app_name,
                        start_date,
                        end_date,
                        created_date,
                        tz_offset,
                        zsource_pk,
                    ),
                )

        print(f"Sanitized fixture generated: {len(rows)} rows -> {out}")
        return 0
    finally:
        src_conn.close()
        out_conn.close()


if __name__ == "__main__":
    raise SystemExit(main())
