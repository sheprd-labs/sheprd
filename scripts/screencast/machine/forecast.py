"""Five-day forecast for a city, from the sample data in days.json."""
import argparse
import json
from pathlib import Path


def fetch(city):
    data = json.loads(Path(__file__).with_name("days.json").read_text())
    return data[city.lower()]


def show(days):
    for day in days:
        print(f"{day['date']}  {day['high']}°/{day['low']}°  {day['sky']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("city")
    args = parser.parse_args()
    show(fetch(args.city))


if __name__ == "__main__":
    main()
