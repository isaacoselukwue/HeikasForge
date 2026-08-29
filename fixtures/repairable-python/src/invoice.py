from decimal import Decimal, ROUND_HALF_EVEN


LINE_ITEM_PRECISION = Decimal("0.01")


def round_currency(amount):
    return Decimal(amount).quantize(LINE_ITEM_PRECISION, rounding=ROUND_HALF_EVEN)


def line_total(unit_price, quantity):
    return round_currency(Decimal(str(unit_price)) * Decimal(quantity))


def invoice_total(line_items):
    total = Decimal("0")
    for unit_price, quantity in line_items:
        total += line_total(unit_price, quantity)
    return round_currency(total)
