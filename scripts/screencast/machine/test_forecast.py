import unittest
import forecast


class Forecast(unittest.TestCase):
    def test_five_days(self):
        self.assertEqual(len(forecast.fetch("Paris")), 5)

    def test_case(self):
        self.assertEqual(forecast.fetch("PARIS"), forecast.fetch("paris"))

    def test_fields(self):
        self.assertEqual(set(forecast.fetch("lyon")[0]), {"date", "high", "low", "sky"})

    def test_unknown(self):
        with self.assertRaises(KeyError):
            forecast.fetch("atlantis")


if __name__ == "__main__":
    unittest.main()
