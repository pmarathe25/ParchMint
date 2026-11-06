# Assistant budget

Durable local estimated-usage reservations. Share one `BudgetJournal` across
the application's project windows, reserve before dispatch, and settle only
when trustworthy usage is available. Unknown outcomes keep their reservation.
This process-local journal is not an account-wide or cross-process billing limit.
