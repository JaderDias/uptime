clippy:
	cargo clippy --all -- \
		-D "clippy::all" \
		-D clippy::pedantic \
		-D clippy::cargo \
		-D clippy::nursery \
		-A clippy::multiple_crate_versions \
        -A clippy::future_not_send \
        -A clippy::missing_panics_doc \
        -A clippy::missing_errors_doc \
        -A clippy::significant_drop_tightening