-- Where a URL or local install came from, so it can be checked again later.
-- `update_info` is the AppImage `.upd_info` string, which names a zsync feed.
ALTER TABLE packages ADD COLUMN download_url TEXT;
ALTER TABLE packages ADD COLUMN update_info TEXT;
