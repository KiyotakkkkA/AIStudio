DELETE FROM `provider` WHERE `adapter` = 'deepseek-web';

DELETE FROM `secret_usage`
WHERE `consumer_kind` = 'account'
  AND `consumer_id` IN (SELECT `id` FROM `account` WHERE `adapter` = 'deepseek-web');

DELETE FROM `secret`
WHERE `id` IN (
    SELECT `token_secret_id` FROM `account`
    WHERE `adapter` = 'deepseek-web' AND `token_secret_id` IS NOT NULL
  )
  AND `id` NOT IN (SELECT `secret_id` FROM `secret_usage`);

DELETE FROM `account` WHERE `adapter` = 'deepseek-web';
