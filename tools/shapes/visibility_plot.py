#!/usr/bin/env python3
from matplotlib.ticker import FormatStrFormatter
import pandas as pd
import seaborn as sns
import matplotlib.pyplot as plt


def main():
    df = pd.read_csv("./visibility.tsv", delimiter="\t", header=0, dtype={"benchmark": "string"}).fillna(0.0)
    print(df.dtypes)
    fig, ax = plt.subplots()
    sns.lineplot(data=df, x = "visibility", y = "ratio", hue = "benchmark", ax=ax, palette = sns.color_palette("colorblind"))
    ax.set_xscale("log", base=2)
    ax.xaxis.set_major_formatter(FormatStrFormatter('%d'))
    ax.set_xlim(64, 65536)
    ax.set_ylim(0, 0.6)
    ax.set_xlabel("Visibility (byte)")
    ax.set_ylabel("Ratio")
    fig.savefig("visibility.pdf")
    fig.savefig("visibility.png")

if __name__ == "__main__":
    main()
