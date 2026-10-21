read -d "" M <<EOF || true
body
EOF
cat <<A | grep x > out
b
A
cat <<MOO

# a leading blank line
MOO
cat <<-EOT
	tabbed
	EOT
