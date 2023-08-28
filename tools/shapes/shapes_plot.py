#!/usr/bin/env python3
from matplotlib.ticker import FormatStrFormatter
import pandas as pd
import seaborn as sns
import matplotlib.pyplot as plt

def main():
    df = pd.read_csv("./shapes.tsv", delimiter="\t", header=0).fillna(0.0)
    benchmarks = df.columns[4:]
    for bm_name in benchmarks:
        df[bm_name] = df[bm_name].sort_values(ascending=False).cumsum().values
    # print(df.melt(id_vars=["rank", "reference_pattern"], var_name="benchmark", value_name="value"))
    fig, ax = plt.subplots()
    sns.lineplot(x=range(1, 33), y=df["cumulative_mean"][0:32], color="black", ax= ax)
    for bm_name in benchmarks:
        sns.lineplot(x=range(1, 33), y=df[bm_name][0:32], color=(0.8, 0.8, 0.8), ax= ax)
    ax.set_xscale("log", base=2)
    ax.xaxis.set_major_formatter(FormatStrFormatter('%d'))
    ax.set_xlim(1, 32)
    ax.set_ylim(0, 100)
    ax.set_xlabel("Rank")
    ax.set_ylabel("")
    fig.savefig("shapes.pdf")

if __name__ == "__main__":
    main()
