DELETE FROM shelves WHERE token_hash = X'00';

CREATE UNIQUE INDEX shelves_token_hash ON shelves (token_hash);
