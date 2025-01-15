
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Ping {
    #[prost(uint32, tag="1")]
    pub id: u32,
}


#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Acknowledge {
    #[prost(uint32, tag="1")]
    pub id: u32,
    #[prost(uint32, tag="2")]
    pub req_id: u32
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Status {
    #[prost(uint32, tag="1")]
    pub id: u32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Status {
    Ok = 0,
    Error = 1
}
