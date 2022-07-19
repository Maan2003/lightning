#ifndef LIGHTNING_PLUGINS_BKPR_ONCHAIN_FEE_H
#define LIGHTNING_PLUGINS_BKPR_ONCHAIN_FEE_H

#include "config.h"
#include <ccan/short_types/short_types.h>

struct amount_msat;
struct bitcoin_txid;

struct onchain_fee {

	/* db_id of account this event belongs to */
	u64 acct_db_id;

	/* Transaction that we're recording fees for */
	struct bitcoin_txid txid;

	/* Incremental change in onchain fees */
	struct amount_msat credit;
	struct amount_msat debit;

	/* What token are fees? */
	char *currency;

	/* Timestamp of the event that created this fee update */
	u64 timestamp;

	/* Count of records we've recorded for this tx */
	u32 update_count;
};

#endif /* LIGHTNING_PLUGINS_BKPR_ONCHAIN_FEE_H */
