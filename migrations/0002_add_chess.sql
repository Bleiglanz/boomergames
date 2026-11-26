-- Add Chess as a supported game in player_stats and matches.

ALTER TABLE player_stats
    DROP CONSTRAINT IF EXISTS player_stats_game_check,
    ADD CONSTRAINT player_stats_game_check CHECK (game IN ('pool', 'checkers', 'chess'));

ALTER TABLE matches
    DROP CONSTRAINT IF EXISTS matches_game_check,
    ADD CONSTRAINT matches_game_check CHECK (game IN ('pool', 'checkers', 'chess'));
