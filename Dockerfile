# syntax=docker/dockerfile:1.7
# Derived from frappe/frappe_docker v4.2.0 images/custom/Containerfile.
# One Dockerfile, two products (trivia, accounting): the product is defined by
# products/<produto>/apps.json (BuildKit secret) + products/<produto>/build.env.
#
# Build context = deployment/ (must contain resources/core/... synced from frappe_docker).
# Build via common/build-image.sh, never by hand.
#
# Local changes vs upstream:
#  - nginx/entrypoint COPYs in the final stage (builder cache not busted)
#  - assets moved inside the builder (no duplicated layer)
#  - pipefail, curl -f, per-arch case, optional sha256 for wkhtmltopdf
#  - runtime: libpq5 (not -dev); git/vim/less only with DEBUG_TOOLS
#  - tini as PID 1
#  - FRAPPE_BRANCH has NO default (must be a pinned tag/commit)
#  - BUILD_MANIFEST (resolved commit of every app) baked into the image
#  - nginx on 8080 (non-root); TLS terminated at Cloudflare

ARG PYTHON_VERSION=3.14
ARG DEBIAN_BASE=trixie
# For strict reproducibility, append @sha256:<digest> to this reference.
FROM python:${PYTHON_VERSION}-slim-${DEBIAN_BASE} AS base

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ARG NODE_VERSION=24
ARG NVM_VERSION=0.40.6
ARG BENCH_VERSION=""
ARG DEBUG_TOOLS=false
ARG INSTALL_POSTGRES_CLIENT=false
ARG INSTALL_WKHTMLTOPDF=true
ARG WKHTMLTOPDF_VERSION=0.12.6.1-3
ARG WKHTMLTOPDF_DISTRO=bookworm
ARG WKHTMLTOPDF_SHA256_AMD64=""
ARG WKHTMLTOPDF_SHA256_ARM64=""
ARG INSTALL_CHROMIUM=true

ENV NVM_DIR=/home/frappe/.nvm \
    NVM_SYMLINK_CURRENT=true \
    PYTHONUNBUFFERED=1 \
    PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1
ENV PATH=${NVM_DIR}/current/bin/:${PATH}

# ---- OS packages (runtime) -------------------------------------------------
RUN useradd -u 1000 -g 0 -d /home/frappe -ms /bin/bash frappe \
    && apt-get update \
    && apt-get install --no-install-recommends -y \
        ca-certificates \
        curl \
        nginx \
        tini \
        gettext-base \
        file \
        jq \
        wait-for-it \
        media-types \
        gpg \
        restic \
        mariadb-client \
        libpq5 \
        libldap2 \
        libsasl2-2 \
        libpango-1.0-0 \
        libharfbuzz0b \
        libpangoft2-1.0-0 \
        libpangocairo-1.0-0 \
    && . /etc/os-release \
    && if [ "$VERSION_CODENAME" = "trixie" ]; then \
        apt-get install --no-install-recommends -y tzdata-legacy; \
    fi \
    && if [ "$INSTALL_POSTGRES_CLIENT" = "true" ]; then \
        apt-get install --no-install-recommends -y postgresql-client; \
    fi \
    && if [ "$DEBUG_TOOLS" = "true" ]; then \
        apt-get install --no-install-recommends -y git vim less; \
    fi \
    && rm -rf /var/lib/apt/lists/*

# ---- Node (nvm) + yarn ------------------------------------------------------
RUN mkdir -p "${NVM_DIR}" \
    && curl -fsSL "https://raw.githubusercontent.com/nvm-sh/nvm/v${NVM_VERSION}/install.sh" | bash \
    && . "${NVM_DIR}/nvm.sh" \
    && nvm install "${NODE_VERSION}" \
    && nvm use "${NODE_VERSION}" \
    && nvm alias default "${NODE_VERSION}" \
    && npm install -g yarn \
    && rm -rf "${NVM_DIR}/.cache" /root/.npm \
    && { \
        echo 'export NVM_DIR="/home/frappe/.nvm"'; \
        echo '[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"'; \
        echo '[ -s "$NVM_DIR/bash_completion" ] && . "$NVM_DIR/bash_completion"'; \
    } >> /home/frappe/.bashrc

# ---- wkhtmltopdf (patched Qt) ----------------------------------------------
# No trixie build exists upstream; the bookworm .deb is used. Disable per product
# (build.env: INSTALL_WKHTMLTOPDF=false) when only Chromium is needed for PDFs.
WORKDIR /tmp
RUN if [ "$INSTALL_WKHTMLTOPDF" = "true" ]; then \
        case "$(dpkg --print-architecture)" in \
            amd64) ARCH=amd64; SHA="${WKHTMLTOPDF_SHA256_AMD64}" ;; \
            arm64) ARCH=arm64; SHA="${WKHTMLTOPDF_SHA256_ARM64}" ;; \
            *) echo "unsupported architecture: $(dpkg --print-architecture)" >&2; exit 1 ;; \
        esac; \
        f="wkhtmltox_${WKHTMLTOPDF_VERSION}.${WKHTMLTOPDF_DISTRO}_${ARCH}.deb"; \
        curl -fsSLO "https://github.com/wkhtmltopdf/packaging/releases/download/${WKHTMLTOPDF_VERSION}/${f}"; \
        if [ -n "$SHA" ]; then echo "${SHA}  ${f}" | sha256sum -c -; fi; \
        apt-get update; \
        apt-get install --no-install-recommends -y "./${f}"; \
        rm -f "$f"; \
        rm -rf /var/lib/apt/lists/*; \
    fi

# ---- Chromium (headless PDF) -------------------------------------------------
RUN if [ "$INSTALL_CHROMIUM" != "false" ]; then \
        apt-get update; \
        DEBIAN_FRONTEND=noninteractive apt-get install --no-install-recommends -y chromium-headless-shell; \
        rm -rf /var/lib/apt/lists/*; \
    fi

# ---- bench + nginx non-root fixes ------------------------------------------
WORKDIR /
RUN pip3 install "frappe-bench${BENCH_VERSION:+==${BENCH_VERSION}}" \
    && rm -f /etc/nginx/sites-enabled/default \
    && mkdir -p /etc/nginx/snippets \
    && sed -i '/user www-data/d' /etc/nginx/nginx.conf \
    && ln -sf /dev/stdout /var/log/nginx/access.log \
    && ln -sf /dev/stderr /var/log/nginx/error.log \
    && touch /run/nginx.pid \
    # OpenShift / arbitrary UID compatibility (GID 0, group-writable)
    && chgrp -R 0 /home/frappe /etc/nginx /var/log/nginx /var/lib/nginx /run \
    && chmod -R g=u /home/frappe /etc/nginx /var/log/nginx /var/lib/nginx /run


# =============================================================================
FROM base AS builder

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install --no-install-recommends -y \
        git \
        wget \
        # arm64 binaries
        libcairo2-dev \
        libpango1.0-dev \
        libjpeg-dev \
        libgif-dev \
        librsvg2-dev \
        # psycopg2
        libpq-dev \
        # other
        libffi-dev \
        liblcms2-dev \
        libldap2-dev \
        libmariadb-dev \
        libsasl2-dev \
        libtiff-dev \
        libwebp-dev \
        pkg-config \
        redis-tools \
        rlwrap \
        tk8.6-dev \
        cron \
        # pandas
        gcc \
        build-essential \
        libbz2-dev \
    && rm -rf /var/lib/apt/lists/*

USER frappe

# No default on purpose: build-image.sh refuses an empty/moving ref.
ARG FRAPPE_BRANCH
ARG FRAPPE_PATH=https://github.com/frappe/frappe
# sha256 of apps.json: a changed BuildKit secret does not invalidate the cache by itself.
ARG CACHE_BUST=""

RUN --mount=type=secret,id=apps_json,target=/opt/frappe/apps.json,uid=1000,gid=0 \
    : "${FRAPPE_BRANCH:?FRAPPE_BRANCH e obrigatorio (tag ou commit)}" \
    && : "${CACHE_BUST}" \
    && APP_INSTALL_ARGS="" \
    && if [ -s /opt/frappe/apps.json ]; then \
        APP_INSTALL_ARGS="--apps_path=/opt/frappe/apps.json"; \
    fi \
    && bench init ${APP_INSTALL_ARGS} \
        --frappe-branch="${FRAPPE_BRANCH}" \
        --frappe-path="${FRAPPE_PATH}" \
        --no-procfile \
        --no-backups \
        --skip-redis-config-generation \
        --verbose \
        /home/frappe/frappe-bench \
    && cd /home/frappe/frappe-bench \
    && echo "{}" > sites/common_site_config.json \
    # record the exact commit of every app before .git is removed
    && for d in apps/*/; do printf '%s %s\n' "$(basename "$d")" "$(git -C "$d" rev-parse HEAD)"; done > BUILD_MANIFEST \
    && find apps -name .git -prune -exec rm -rf {} + \
    # sites/ is a volume at runtime: keep built assets in the image layer instead
    && mv sites/assets assets \
    && chmod -R g=u /home/frappe/frappe-bench


# =============================================================================
FROM base AS backend

ARG IMAGE_TITLE="frappe-saas"
ARG IMAGE_SOURCE=""
ARG IMAGE_VERSION=""
ARG IMAGE_REVISION=""
LABEL org.opencontainers.image.title="${IMAGE_TITLE}" \
      org.opencontainers.image.source="${IMAGE_SOURCE}" \
      org.opencontainers.image.version="${IMAGE_VERSION}" \
      org.opencontainers.image.revision="${IMAGE_REVISION}"

USER frappe
COPY --from=builder --chown=frappe:0 /home/frappe/frappe-bench /home/frappe/frappe-bench
WORKDIR /home/frappe/frappe-bench
RUN chmod g=u /home/frappe/frappe-bench

VOLUME [ \
    "/home/frappe/frappe-bench/sites", \
    "/home/frappe/frappe-bench/logs" \
]

# Small, frequently-edited files last (they only invalidate the layers below).
COPY --chmod=644 resources/core/nginx/nginx-template.conf    /templates/nginx/frappe.conf.template
COPY --chmod=644 resources/core/nginx/security_headers.conf  /etc/nginx/snippets/security_headers.conf
COPY --chmod=755 resources/core/nginx/nginx-entrypoint.sh    /usr/local/bin/nginx-entrypoint.sh
# Links image-built assets into the mounted sites volume at container start.
COPY --chmod=755 resources/core/main-entrypoint.sh           /usr/local/bin/entrypoint.sh
COPY --chmod=755 resources/core/start.sh                     /usr/local/bin/start.sh

# 8080 nginx (the only port cloudflared needs), 8000 gunicorn, 9000 socketio
EXPOSE 8080 8000 9000

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh"]
CMD ["start.sh"]
