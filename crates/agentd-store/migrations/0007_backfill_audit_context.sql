UPDATE audit_events
SET
    trace_id = COALESCE(NULLIF(trace_id, ''), 'trace-' || id),
    session_id = COALESCE(NULLIF(session_id, ''), 'session-' || id),
    severity = COALESCE(
        NULLIF(severity, ''),
        CASE result
            WHEN 'failure' THEN 'error'
            WHEN 'pending' THEN 'warning'
            WHEN 'cancelled' THEN 'warning'
            ELSE 'info'
        END
    );
