use crate::resolver::Resolver;
use async_trait::async_trait;
use trust_dns_resolver::{
    lookup,
    proto::rr::{Record, RecordType},
};
use trust_dns_server::{
    authority::{Authority, LookupError, LookupObject},
    client::rr::LowerName,
};

#[derive(Clone)]
pub struct Forwarder {
    pub resolver: Resolver,
    pub domain_name: LowerName,
}

#[async_trait]
impl Authority for Forwarder {
    type Lookup = ForwardLookup;

    fn zone_type(&self) -> trust_dns_server::authority::ZoneType {
        trust_dns_server::authority::ZoneType::Forward
    }

    fn is_axfr_allowed(&self) -> bool {
        false
    }

    async fn update(
        &self,
        _update: &trust_dns_server::authority::MessageRequest,
    ) -> trust_dns_server::authority::UpdateResult<bool> {
        Ok(false)
    }

    fn origin(&self) -> &trust_dns_server::client::rr::LowerName {
        &self.domain_name
    }

    async fn lookup(
        &self,
        name: &trust_dns_server::client::rr::LowerName,
        rtype: RecordType,
        _lookup_options: trust_dns_server::authority::LookupOptions,
    ) -> Result<Self::Lookup, LookupError> {
        let lookup = self.resolver.lookup(name, rtype).await;

        lookup.map(ForwardLookup).map_err(LookupError::from)
    }

    async fn search(
        &self,
        request_info: trust_dns_server::server::RequestInfo<'_>,
        lookup_options: trust_dns_server::authority::LookupOptions,
    ) -> Result<Self::Lookup, LookupError> {
        self.lookup(
            request_info.query.name(),
            request_info.query.query_type(),
            lookup_options,
        )
        .await
    }

    async fn get_nsec_records(
        &self,
        _name: &trust_dns_server::client::rr::LowerName,
        _lookup_options: trust_dns_server::authority::LookupOptions,
    ) -> Result<Self::Lookup, LookupError> {
        Err(LookupError::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            "this kind of lookup is not supported",
        )))
    }
}

pub struct ForwardLookup(lookup::Lookup);

impl LookupObject for ForwardLookup {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = &'a Record> + Send + 'a> {
        Box::new(self.0.record_iter())
    }

    fn take_additionals(&mut self) -> Option<Box<dyn LookupObject>> {
        None
    }
}
