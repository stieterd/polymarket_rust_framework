import requests
import pandas as pd
import market_data
import json
import traceback
import relay_converter
import time

events = {}

def extract_index_from_question_id(question_id: str) -> int:
    """
    Given a hex questionID like "0x6df9c3...dda02", returns 2
    (the low-order byte of the 32-byte value).
    """
    # drop the "0x", parse into a big int, and mask off the low byte
    return int(question_id, 16) & 0xFF

while True:
    print("just slept")
    url = "https://data-api.polymarket.com/positions?user=0xb48b9192DC52eED724Fa58c66Fa8926d06A3648e&sizeThreshold=.1&limit=500&offset=0&sortBy=CURRENT&sortDirection=DESC"

    r = requests.get(url)

    df = pd.DataFrame(r.json())
    # df.to_csv("my_csv.csv", sep=',')
    df = df[df['outcomeIndex']==1]
    df['eventSlugcounts'] = df.groupby('eventSlug')['eventSlug'].transform('count')

    
    # Filter eventSlugs that appear more than twice
    df = df[df['eventSlugcounts'] >= 2]

    unique_urls = []
    try:
        with open("sample_json.json", 'r') as reader:
            markets_json = json.load(reader)

        markets_json = market_data.get_market_data()

        events_grouped = df[df['outcome']=="No"].groupby("eventSlug").filter(lambda g: len(g) >= 2).groupby("eventSlug")
        time.sleep(10)
        url = "https://data-api.polymarket.com/positions?user=0xb48b9192DC52eED724Fa58c66Fa8926d06A3648e&sizeThreshold=.1&limit=500&offset=0&sortBy=CURRENT&sortDirection=DESC"

        r = requests.get(url)

        df2 = pd.DataFrame(r.json())
        df2 = df2[df2['outcomeIndex']==1]
        df2['eventSlugcounts'] = df2.groupby('eventSlug')['eventSlug'].transform('count')

        
        # Filter eventSlugs that appear more than twice
        df2 = df2[df2['eventSlugcounts'] >= 2]
        
        for event_slug, event in (events_grouped):
            
            # we want to merge
            # if event_slug == "ballon-dor-winner-2025":
            #     continue

            event_df = (df2[df2["eventSlug"]==event_slug])
            # print([item['slug'] for item in markets_json['data']])
            try:
                event = next(item for item in markets_json if item['slug'] == event_slug)
            except Exception as e:
                continue 
            market_id = event['negRiskMarketID']
            indices = []

            amount = event_df["size"].min()

            # if amount < 5:
            #     continue

            for market_slug, market in event_df.groupby("slug"):
                pdmarkets_df = pd.json_normalize(event, record_path='markets')
                idx = extract_index_from_question_id(pdmarkets_df[pdmarkets_df['slug'] == market_slug]["questionID"].iloc[0])
                # idx = int(suffix, 16)                  # e.g. 0x02 → 2
                indices.append(idx)
                # indices.append(int(pdmarkets_df[pdmarkets_df['slug'] == market_slug]['groupItemThreshold'].iloc[0]))

            if (amount, market_id) in events and time.time() - events[(amount, market_id)] < 3:
                continue
            
            keys_to_del = []
            for key in events.keys():
                if market_id in key:
                    keys_to_del.append(key)

            for key in keys_to_del:
                del events[key]

            events[((amount, market_id))] = time.time()

            print(relay_converter.convert_tokens_negrisk(market_id, relay_converter.positions_to_index_set(indices), (amount)))
            time.sleep(2)
    except Exception as e:
        print(traceback.format_exc())
        pass

    
