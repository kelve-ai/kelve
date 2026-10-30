-- Ideas below one someone opened were traced on down by themselves, and
-- crowded out reading. Now nothing is traced that no one asked for: what
-- was queued that way is let go. Opening one of those ideas asks again.
update asks set state = 'dropped'
 where kind = 'relate' and state = 'waiting'
   and json_extract(resume_payload, '$.below') = 1;
delete from jobs
 where kind = 'entry.relate' and locked_at is null
   and json_extract(payload, '$.below') = 1;
