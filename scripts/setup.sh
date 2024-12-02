#!/usr/bin/env bash

. /usr/local/config/config.config

# Secure certs
echo -e "Writing certs"
echo "$SITE_CERT" >> /etc/ssl/certs/site.crt
chown root:root /etc/ssl/certs/site.crt
echo "$SITE_KEY" >> /etc/ssl/private/site.key
chown root:root /etc/ssl/private/site.key
chmod 600 /etc/ssl/private/site.key

# Setup site
echo -e "Setting up site on port $PORT at $DOMAIN"
cp /usr/local/config/sites-available/*.conf /etc/nginx/sites-available/
sed -i -e "s/{DOMAIN}/$DOMAIN/g" /etc/nginx/sites-available/*.conf
sed -i -e "s/{PORT}/$PORT/g" /etc/nginx/sites-available/*.conf
rm /etc/nginx/sites-enabled/default
ln -s /etc/nginx/sites-available/www.site.conf /etc/nginx/sites-enabled/www.site.conf

# Setup surrealdb
curl -sSf https://install.surrealdb.com | sh

echo -e "Running surrealdb"
surreal start --user root --pass root "rocksdb:/usr/local/db" 1>&2 &
sleep 5

#nginx &

exec /usr/local/backend "0.0.0.0:$PORT" /usr/local/site 1>&2
echo -e "Done!"