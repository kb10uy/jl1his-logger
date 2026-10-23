UPDATE stations SET fields = json_remove(fields, '$.MY_RIG', '$.MY_ANTENNA', '$.TX_PWR')
WHERE json_type(fields, '$.MY_RIG') IS NOT NULL
   OR json_type(fields, '$.MY_ANTENNA') IS NOT NULL
   OR json_type(fields, '$.TX_PWR') IS NOT NULL;
