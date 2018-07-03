# BitCharge - Payment page for accepting bitcoins

BitCharge is a small web app that allows your clients to pay you in bitcoins, which are then automatically converted to euros and deposited to your bank account. The conversion is handled by the [Coinmotion](https://coinmotion.com/) exchange.

![Screenshot of BitCharge in action](https://raw.githubusercontent.com/roosmaa/bitcharge-rs/master/screenshot.png)

This software is useful for freelancers and small companies:

- ... who invoice their clients manually
- ... who reside in the eurozone
- ... who are tech savvy (managing a virtual server, comfortable with command-line and ssh)

BitCharge is an alternative to BitPay, CoinPayments and other SaaS offerings. Those services are more tailored towards the retail sector. For a small company it doesn't make sense to pay their fees nor the extra fees of the bitcoin network as money is moved around.

It is still very early days of BitCharge, but it currently has the following features:

- Clean payment instructions page for the payer
- Quoting a bitcoin amount based on the live EUR/BTC exchange rates
- Funds are paid directly to your Coinmotion account, avoiding unnecessary fees
- Automatically covert bitcoins to euros and withdraw them immediately via the Coinmotion API

## Setup instructions

- Make sure you have Rust programming language installed ([instructions](https://www.rust-lang.org/en-US/install.html))
- Clone this repository
- Run `cargo build --release`
- Copy `bitcharge.toml.example` to `bitcharge.toml`
- Use your preferred text editor to edit `bitcharge.toml` and fill out the required fields
- Start the app `RUST_LOG=info ./target/release/bitcharge`

You should also setup Nginx/Apache with HTTPS termination in front of BitCharge.

### Registering invoices with BitCharge

Registering invoices with BitCharge is somewhat tedious currently, but it will get better in the future.

- Open Coinmotion web interface, go to Receive tab and generate a new deposit address using your invoice number as the description
- Open the `bitcharge.toml` in your preferred text editor and add a new `[[charges]]` section to it (template below), filling it in with the appropriate information and your newly generated deposit address
- Restart the _bitcharge_ process and monitor the logs for the list of charges to get the public URL associated with your new charge

Template for the new charge to be used in the `bitcharge.toml` file:

```
[[charges]]
# The ID should be incremented for each new charge (1, 2, 3, ...)
id = 1
# The unique identifier on your invoices
invoice_id = "2018-0012"
# The amount in euros that the invoice is made out to be
eur_amount = "1234.56"
# The new deposit address from Coinmotion
btc_address = ""
```
