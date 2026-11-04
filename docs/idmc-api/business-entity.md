<!-- source: Business_entity_record_APIs.html -->
# Business entity record APIs

# Business entity record APIs

Use the business entity record APIs to read, create, update, delete, or restore master records. These APIs use the entity resource.

Ensure that you have the required privileges to create, update, read, or delete a record.

**Note:** 

If you don't have the read privilege for a field, the response of Business Entity Record REST APIs don't include the field.

For more information about record and field privileges, see [Configure field privileges for business application user roles](https://www4.informatica.com/onlinehelp/default.aspx?product=IICS/prod&ver=b360&lang=en&contextID=FIELD_PRIVILEGES) and [Configure record privileges for business application user roles](https://www4.informatica.com/onlinehelp/default.aspx?product=IICS/prod&ver=b360&lang=en&contextID=RECORD_PRIVILEGES)

## Create Master Record

Use the Create Master Record API to create a master record for a business entity.

The API supports the POST method.

### POST request

To create a business entity master record, submit a POST request with the following URI:

POST <baseApiURL>/business-entity/public/api/v1/entity/<businessEntity>?sourceSystem=<sourceSystem>[&sourcePKey=<pkey>][&\_resolveCrosswalk=true|false]  
[&businessId=999456789222123][&\_population=<value>]

The following table describes the parameters that you can use in the URI:

| Parameter Name | Description |
| --- | --- |
| businessEntity | Internal ID of the business entity for which you want to create a master record. |
| sourceSystem | Internal ID of the source system to which the record belongs. |
| sourcePKey | The primary key of the source record.  The source primary keys of records can contain the following characters:  ~!'={}|:@#$^&\*()-\_+,<>?` |
| \_resolveCrosswalk | Indicates whether to standardize picklist values based on the source system configuration.  To standardize the picklist values, set to true. If set to true, ensure that the picklist values belong to a code list that's part of a crosswalk.  Default is false.  For more information about standardizing picklist values, see Define Source Systems. |
| businessId | Unique identifier of the business entity record.  When you specify a custom business ID for records, ensure that its length differs from the length of the business ID format of the business entity. For example, if you specified 111 as the prefix and 15 as the length in the business ID format for a business entity, don't use 111560000800000 as a custom business ID. However, you can use a custom business ID with the same prefix, 111, provided the overall length is different.  For more information about the business ID format, see [Business ID format](https://www4.informatica.com/onlinehelp/default.aspx?product=IICS/prod&ver=b360&lang=en&contextID=BUSINESS_ID_FORMAT).  **Note:**  If a record contains more than 2,000 field group entries and 23 data quality rule associations, the request might time out. |
| \_population | Specifies the population of the record. Match keys for the record are generated based on the population that you assign to improve match accuracy.  You can assign a population such as usa to a record to generate match keys based on the record data, such as name or address, that account for USA data characteristics.  If you don't specify the population of the record, the population that's configured for the match model is applied to the record.  For more details about the values that you can specify for the parameter, see [Supported Population](https://www4.informatica.com/onlinehelp/default.aspx?product=IICS/prod&ver=b360&lang=en&contextID=SUPPORTED_POP). |

### Request body

You must include the record details in the request body. To view the request body parameters, submit a GET request and retrieve an active business entity master record.

If a business entity field in a field group has a data quality rule, specify the \_id parameter value for the field group entry that you create. For example, if the Postal Code field in the Address field group has a data quality rule, specify an ID for an address entry that you create.

For more information about data quality rules, see Configure Data Quality.

If a business entity is configured with dynamic field definition and dynamic field value attributes, you can configure values for these attributes when you create or update master records.

The following table describes the parameters that you can use to add or update dynamic field definitions:

| Parameter Name | Description |
| --- | --- |
| <Internal ID of a dynamic field definition attribute> | An array of dynamic fields from the dynamic pool that's associated with the dynamic field definition attribute.  Use the following parameters to add a dynamic field definition:   * \_id * template * mandatory * inheritance > mode |
| \_id | Unique identifier for the dynamic field definition entry. |
| template | Name of a dynamic field from the dynamic pool that's associated with the dynamic field definition attribute. |
| mandatory | Optional. Specifies whether the dynamic field is required. Default is false. |
| inheritance > mode | Optional. Controls inheritance of a dynamic field definition.  Specify one of the following modes:   * NONE. Adds a dynamic field definition to the record without enabling inheritance for the definition. This prevents the record from propagating the dynamic field definition to its child records. Default is NONE. * START. Enables the inheritance of a dynamic field definition from the record to its child records. The inheritance of the definition starts from the record. For example, when you add the Color dynamic field definition to the Mobile Phones category record with the START mode, the child records of the Mobile Phones record get the Color dynamic field definition. Any changes to the definition are also propagated to the child records. * PASS: The record inherits the dynamic field definition from its parent record and propagates it to the child records. * STOP. Stops the inheritance chain for a definition at the record. The record continues to inherit modifications to the definition. If the child records inherited the definition previously, they will no longer get the definition. * DISABLED. Hides the dynamic field definition in the user interface for the record. The child records can't inherit the definition. |

**Note:** 

Even if you don't specify the required dynamic field values in a request, the record is created or updated without any validation.

### POST response

The response is in the JSON format. The response shows the HTTP 200 OK response code and retrieves the business ID of the record you created. When you submit a REST API request to create a record, MDM SaaS verifies the permissions of your user role and the approval workflow configuration. The response indicates whether a data steward must approve the records you created.

**Note:** 

The response contains the id parameter that's deprecated. Use the businessId parameter instead.

### POST example

You might use the following request to create a master record for the c360.organization business entity:

POST <baseApiUrl>/business-entity/public/api/v1/entity/c360.organization?sourceSystem=c360.default.system&sourcePKey=pkey910&\_resolveCrosswalk=true  
&businessId=999456789222123  
Content-Type: application/json  
IDS-SESSION-ID: XXXXXXXXXXXXXXXXXXXXXX   
  
  
{  
  
 "firstName": "Lewis",  
 "lastName": "Hamilton",  
 "middleName": "Decay",  
 "PostalAddress": [  
 {  
 "\_id": "3d4a8f4d07cb4f0db9ea1be9bf8d27e8",  
 "defaultIndicator": true,  
 "addressType": {  
 "Name": "Home",  
 "Code": "Home"  
 },  
 "usageType": {  
 "Name": "Home",  
 "Code": "Home"  
 },  
 "addressStatus": {  
 "Name": "Active",  
 "Code": "Active"  
 },  
 "addressLine1": "789456, park road",  
 "country": {  
 "Name": "United States",  
 "Code": "US"  
 },  
 "state": {  
 "Name": "California",  
 "Code": "CA"  
 },  
 "city": "California"  
 }  
 ],  
 "Phone": [  
 {  
 "\_id": "f142a11b916c4a1b8239466f15852",  
 "phoneUsageType": {  
 "Name": "Home",  
 "Code": "Home"  
 },  
 "phoneType": {  
 "Name": "Mobile",  
 "Code": "Mobile"  
 },  
 "phoneNumber": "9900202777"  
 }  
 ],  
 "\_contentMeta": {  
"trust": {  
"fieldData": [  
{  
"fieldRef": "/",  
"sourceLastUpdateDate": 1654058410000  
},   
{  
"fieldRef": "PostalAddress[3d4a8f4d07cb4f0db9ea1be9bf8d27e8]",  
"sourceLastUpdateDate": 1654058410000  
},  
{  
"fieldRef": "Phone[f142a11b916c4a1b8239466f15852]",  
"sourceLastUpdateDate": 1654839671000  
}  
]  
  
}  
 }  
}

You might use the following excerpt in the request body to create a master record with dynamic field definitions:

{  
 "identifier" : "SmartPhones",  
 "categoryName": "Smart Phones",  
 "product\_dynamic\_field\_definitions" : [  
 {  
 "\_id": "color\_001\_dfd1",  
 "template": "color",  
 "mandatory": true,  
 "allowMany": true,  
 "inheritance": [  
 {  
 "mode": "NONE"  
 }  
 ]  
 },  
 {  
 "\_id": "dimensions\_001\_dfd1",  
 "template": "dimensions",  
 "mandatory": true,  
 "allowMany": true,  
 "inheritance": [  
 {  
 "mode": "NONE"  
 }  
 ]  
 }  
 ]  
}

In the sample request, product\_dynamic\_field\_definitions is the internal ID of a dynamic field definition attribute in a business entity. The \_id parameter contains a unique identifier for each dynamic field definition entry. The template parameter specifies a dynamic field from the dynamic field pool that's associated with the dynamic field definition attribute.

You might use the following request to create a master record with dynamic field values:

{  
 "identifier" : "iPhone17Pro",  
 "GTN": "GHN456",  
 "brand": "Apple",  
 "product\_description": "<p>The <strong>iPhone 17 Pro</strong> is a high-performance smartphone featuring an A19 Pro chip, a Super Retina XDR display with ProMotion, and an upgraded camera system with 48MP sensors for the main, ultrawide, and telephoto lenses. </p>",  
 "product\_specifications": "<ul><li>Super Retina XDR display</li><li>15.93 cm / 6.3â³ (diagonal)</li><li>Allâscreen OLED display</li><li>2622 Ã 1206 pixel resolution at 460 ppi</li></ul>",  
 "product\_dynamic\_field\_values" : {  
 "color" : "red",  
 "dimensions" : [  
 {  
 "height" : "15.93",  
 "width" : "6.3"  
 }  
 ]  
 }  
}

In the sample request, dynamic\_field\_values is the internal ID of a dynamic field values attribute in a business entity.

**Note:** 

Ensure that you specify the dynamic fields from the dynamic field pool that's associated with the dynamic field definition attribute. If you include rich text in your request, ensure that you use the supported HTML tags and attributes. For more information about rich text formatting, see Rich text formatting for text fields.

If no approval workflow is configured, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired":false,  
 "id":"9991234567890123",  
 "businessId:"999456789222123"  
}

If the approval workflow is configured, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired":true,  
 "id":"9991234567890123",  
 "businessId:"999456789222123"  
}

## Read Master Record by Business ID

Use the Read Master Record by Business ID API to retrieve an active business entity master record based on the business ID of the record.

To get the business ID of a record for use with the API, you can submit a search request.

The API supports the GET method.

### GET request

To retrieve a business entity master record, submit a GET request with the following URI:

GET <baseApiURL>/business-entity/public/api/v1/entity/<businessEntity>/<businessId>/[&\_skipLookup=true|false]?[\_showContentMeta=true|false][&\_showPending=true|false]

The GET request contains the following parameters that you can use in the URI:

| Name | Description |
| --- | --- |
| businessEntity | Internal ID of the business entity from which you want to retrieve the master record. |
| businessId | Unique identifier of the business entity record. |
| \_skipLookup | Optional. Indicates whether to retrieve picklist values from the Business 360 data store or Reference 360 data store. The response time improves when you retrieve values from the Business 360 data store.  To retrieve picklist values from the Business 360 data store, set to true. The picklist values are based on the data from the last updated record and might not include subsequent changes to the Reference 360 data. To retrieve picklist values from the Reference 360 data store, set to false. Default is false. |
| \_showContentMeta | Optional. Indicates whether to retrieve content metadata of a record. The content metadata contains the trust score, survivorship configuration, and details of any failed data enhancement rule.  To retrieve content metadata of a record, set \_showContentMeta to true. Default is false. |
| \_showPending | Optional. Indicates whether to retrieve only master records that are pending review for record creation, update, or deletion.  To retrieve pending records, set to true. Default is false. |

### GET response

The response is in the JSON format. The response body contains the business entity master record data.

The GET response includes the following \_meta parameters:

| Name | Type | Description |
| --- | --- | --- |
| id | String | Unique identifier of the business entity record. Do not use. |
| businessEntity | String | Internal ID of the business entity from which you retrieved the master record. |
| businessId | String | Unique identifier of the business entity record. |
| state | String | Indicates whether the record is active, inactive, or in pending state.  **Note:**  This parameter is the same as the status parameter in the search API. For more information about the status parameter, see Request response . |
| validation | String | Indicates the validation status of the record.  The record can have one of the following statuses:   * PENDING. Indicates that the record is not yet validated. * PASSED. Indicates that the record is validated and has no errors. * FAILED. Indicates that the record is validated and has validation and data quality errors. * IN\_PROGRESS. Indicates that the record is being validated. |
| consolidation | String | Match and merge status of the master record. |
| searchIndex | String | Indicates that the record is in the SEARCH\_INDEXED state and is searchable. |
| createdBy | String | Name of the person who created the record. |
| creationDate | String | Date when the record was created. |
| updatedBy | String | Name of the user who last updated the record. |
| lastUpdateDate | String | Date when the master record was last updated. |
| changeList | String | ID of the workflow task that's associated with the record in the pending state. |

The GET response includes the following \_contentMeta parameters:

| Name | Type | Description |
| --- | --- | --- |
| trust | Object | An object with the fieldData array that contains details about the fields participating in the trust score calculation.  The fieldData array contains the following parameters:   * fieldRef. Fields that have different trust scores. The value "/" indicates that all fields in the record share the same trust score. * lastUpdatedDate. Date when the field value was last updated. * manualOverride. Indicates whether a field value was manually overridden. * sourcePrimaryKey. Primary key of the source record. * sourceSystem. Internal ID of the source system to which the record belongs. * trustRule. Survivorship configuration that identified the best value for the field. * trustScore. Latest trust score of the field value. |
| ruleAssociation | Object | An object that contains details of failed or in-progress rule associations.  The object includes the fail array that contains details about the failed or in-progress rule associations and the fields to which these rules were applied.  The fail array contains the following parameters:   * provider. For internal use only. * rule. Array that contains details about failed or in-progress rule associations.  The array contains the following parameters:   + id. Internal ID of the failed or in-progress rule association. + fieldRef. Array that contains the fields to which the rule associations were applied. + dynErrorDetail. Error message that appears when a custom rule specification is configured from Data Quality. |

### GET examples

You might use the following request to retrieve the active master record based on the business ID MDM0000000GXKU:

GET <baseApiURL>/business-entity/public/api/v1/entity/c360.person/MDM0000000GXKU?\_showContentMeta=true

The sample response shows the HTTP 200 OK response code and retrieves the business entity master record that matches the specified business ID. The response shows the content metadata in the \_contentMeta section.

{  
 "firstName": "Andrew",  
 "middleName": "Xio",  
 "lastName": "Plata",  
 "fullName": "Andrew Xio Plata",  
 "gender": {  
 "Code": "M",  
 "Name": "Male"  
 },  
 "\_meta": {  
 "id": "MDM0000000GXKU",  
 "businessEntity": "c360.person",  
 "businessId": "MDM0000000GXKU",  
 "state": "ACTIVE",  
 "validation": "IN\_PROGRESS",  
 "consolidation": "MATCH\_INDEXED",  
 "searchIndex": "SEARCH\_INDEXED",  
 "createdBy": "Allied",  
 "creationDate": 1748510430175,  
 "updatedBy": "Allied",  
 "lastUpdateDate": 1748510430175  
 },  
 "\_contentMeta": {  
 "trust": {  
 "fieldData": [  
 {  
 "fieldRef": "middleName",  
 "lastUpdatedDate": 1748510430175,  
 "manualOverride": false,  
 "sourcePrimaryKey": "683826de176d8555c1172fd7",  
 "sourceSystem": "c360.default.system",  
 "trustRule": "SourceRanking",  
 "trustScore": 0.9  
 },  
 {  
 "fieldRef": "/",  
 "lastUpdatedDate": 1748510430175,  
 "manualOverride": false,  
 "sourcePrimaryKey": "683826de176d8555c1172fd7",  
 "sourceSystem": "c360.default.system",  
 "trustRule": "SourceRanking",  
 "trustScore": 1.0  
 }  
 ]  
 },  
 "ruleAssociation": {  
 "fail": [  
 {  
 "provider": "mdm-business-entity",  
 "rule": [  
 {  
 "id": "min5\_middlename\_1743147609745",  
 "fieldRef": [  
 "middleName"  
 ],  
   
 "dynErrorDetail": "Requires a minimum of five alphabetic characters."  
 }  
 ]  
 }  
 ],  
 "exception": []  
 }  
 }  
}

For more information about content metadata, see [Read content meta](Read_content_meta.md "Read content meta") .

The following sample response returns a record that contains dynamic field definitions:

{  
 "identifier": "Electronics",  
 "category\_name": "Electronics",  
 "product\_dynamic\_field\_definitions": [  
 {  
 "\_id": "color\_property\_01",  
 "template": "color",  
 "mandatory": false,  
 "allowMany": true,  
 "inheritance": [  
 {  
 "\_id": "6835ab2ca1d3e6000f749d84",  
 "mode": "START"  
 }  
 ]  
 }  
 ],  
 "\_meta": {  
 "id": "MDM00000000FFL",  
 "businessEntity": "custom\_category",  
 "businessId": "MDM00000000FFL",  
 "state": "ACTIVE",  
 "validation": "PASSED",  
 "consolidation": "MATCH\_DIRTY",  
 "searchIndex": "SEARCH\_INDEXED",  
 "createdBy": "Marexus\_S10\_Mar\_11",  
 "creationDate": 1748341536222,  
 "updatedBy": "Marexus\_S10\_Mar\_11",  
 "lastUpdateDate": 1748347692193  
 }  
}

The following sample response returns a record that contains dynamic field values:

{  
 "identifier": "JBL Cinema SB510",  
 "gtn": "SB510",  
 "brand": "JBL",  
 "product\_number": "CINEMA SB510",  
 "product\_name": "JBL Cinema SB510",  
 "product\_description": "<p>The <strong>JBL Cinema SB510</strong> is a 3.1 channel all-in-one soundbar with a built-in subwoofer. It delivers 200 watts of powerful Dolby Audio with deep bass, all in a hassle-free, compact design that fits neatly into any room. A dedicated center channel driver means TV and film dialogue is always crystal clear. Setup is easy using either one-cable HDMI ARC or optical connections, and convenient Bluetooth connectivity means you can use it for wireless music streaming from any device.</p>",  
 "product\_specifications": "<ul><li>Model: CINEMA SB510</li><li>Sound system: 3.1 channel</li><li>Power supply: 100â240 V AC, ~50/60 Hz</li><li>Total speaker power output (Max @ THD 1%): 200 W</li><li>Soundbar output power (Max @ THD 1%): 150 W</li><li>Subwoofer output power (Max @ THD 1%): 50 W</li><li>Soundbar transducer: 3 Ã (45 Ã 80) mm racetrack drivers</li><li>Subwoofer transducer: 1 Ã (70 Ã 88) mm built-in subwoofer</li><li>Standby power: &lt; 0.5 W</li><li>Operating temperature: 0Â°Câ45Â°C</li></ul>",  
 "status": {  
 "Code": "active",  
 "Name": "Active"  
 },  
 "availability": {  
 "Code": "in\_stock",  
 "Name": "In Stock"  
 },  
 "product\_dynamic\_field\_values": {  
 "dolby\_atmos": true,  
 "color": "Red",  
 "dimensions": [  
 {  
 "\_id": "dbb63393e71b424e8873d0e921622e1e",  
 "width": "8.6 cm",  
 "height": " 7.12 cm"  
 }  
 ]  
 },  
 "\_meta": {  
 "id": "MDM00000000000",  
 "businessEntity": "customitem",  
 "businessId": "MDM00000000000",  
 "state": "ACTIVE",  
 "validation": "PASSED",  
 "consolidation": "MATCH\_DIRTY",  
 "searchIndex": "SEARCH\_INDEXED",  
 "createdBy": "Marexus\_S10\_Mar\_11",  
 "creationDate": 1748345857233,  
 "updatedBy": "Marexus\_S10\_Mar\_11",  
 "lastUpdateDate": 1748501231546  
 }  
}

You might use the following request to retrieve the pending master record based on the business ID MDM0000000PXN5:

GET <baseApiURL>/business-entity/public/api/v1/entity/c360.person/MDM0000000PXN5?\_showContentMeta=true&\_showPending=true

The sample response shows the HTTP 200 OK response code and retrieves the pending master record that matches the specified business ID. The response includes the changeList parameter in the \_meta section.

{  
 "firstName": "Anita",  
 "middleName": "Jane",  
 "lastName": "Kumar",  
 "fullName": "Anita Jane Kumar",  
 "\_meta": {  
 "id": "MDM0000000PXN5",  
 "businessEntity": "c360.person",  
 "businessId": "MDM0000000PXN5",  
 "state": "PENDING\_CREATE",  
 "validation": "PASSED",  
 "consolidation": "MATCH\_INDEXED",  
 "searchIndex": "SEARCH\_INDEXED",  
 "createdBy": "Impact",  
 "creationDate": 1756473570942,  
 "updatedBy": "Impact",  
 "lastUpdateDate": 1756473570942,  
 "changeList": "68b1a8e24fb65c72f80df3c0"  
 },  
 "\_contentMeta": {  
 "trust": {  
 "fieldData": [  
 {  
 "fieldRef": "middleName",  
 "lastUpdatedDate": 1756473570942,  
 "manualOverride": false,  
 "sourcePrimaryKey": "683826de176d8555c1172fd7",  
 "sourceSystem": "c360.default.system",  
 "trustRule": "SourceRanking",  
 "trustScore": 0.9  
 },  
 {  
 "fieldRef": "/",  
 "lastUpdatedDate": 1748510430175,  
 "manualOverride": false,  
 "sourcePrimaryKey": "683826de176d8555c1172fd7",  
 "sourceSystem": "c360.default.system",  
 "trustRule": "SourceRanking",  
 "trustScore": 1.0  
 }  
 ]  
 }  
 }  
}

## Read Master Record by SourcePKey

Use the Read Master Record by SourcePKey API to retrieve an active business entity master record associated with a particular source system and the primary key.

The API supports the GET method.

### GET request

To retrieve a business entity master record, submit a GET request with the following URI:

GET <baseApiURL>/business-entity/public/api/v1/entity/<businessEntity>/<sourceSystem>/<sourcePKey>/[&\_skipLookup=true|false]?[\_showContentMeta=true|false][&\_showPending=true|false]

The GET request contains the following parameters that you can use in the URI:

| Parameter | Description |
| --- | --- |
| businessEntity | Internal ID of the business entity for which you want to retrieve the master record. |
| sourceSystem | Internal ID of the source system to which the record belongs. |
| sourcePKey | The primary key of the source record. |
| \_skipLookup | Optional. Indicates whether to retrieve picklist values from the Business 360 data store or Reference 360 data store. The response time improves when you retrieve values from the Business 360 data store.  To retrieve picklist values from the Business 360 data store, set to true. The picklist values are based on the data from the last updated record and might not include subsequent changes to the Reference 360 data. To retrieve picklist values from the Reference 360 data store, set to false. Default is false. |
| \_showContentMeta | Optional. Indicates whether to retrieve content metadata of a record. The content metadata contains the trust score, survivorship configuration, and details of any failed data enhancement rule.  To retrieve content metadata of a record, set \_showContentMeta to true. Default is false. |
| \_showPending | Optional. Indicates whether to retrieve only master records that are pending review for record creation, update, or deletion.  To retrieve pending records, set to true. Default is false. |

### GET response

The response is in the JSON format. The response body contains the business entity master record data.

The GET response includes the following \_meta parameters:

| Name | Type | Description |
| --- | --- | --- |
| id | String | Unique identifier of the business entity record. Do not use. |
| businessEntity | String | Internal ID of the business entity from which you retrieved the master record. |
| businessId | String | Unique identifier of the business entity record. |
| state | String | Indicates whether the record is active, inactive, or in pending state.  **Note:**  This parameter is the same as the status parameter in the search API. For more information about the status parameter, see Request response . |
| validation | String | Indicates the validation status of the record.  The record can have one of the following statuses:   * PENDING. Indicates that the record is not yet validated. * PASSED. Indicates that the record is validated and has no errors. * FAILED. Indicates that the record is validated and has validation and data quality errors. * IN\_PROGRESS. Indicates that the record is being validated. |
| createdBy | String | Name of the person that created the record. |
| creationDate | String | Date when the record was created. |
| updatedBy | String | Name of the person that updated the record. |
| lastUpdateDate | String | Date when the record was last updated. |
| changeList | String | ID of the workflow task associated with the record, if it's in the pending state. |

The GET response includes the following \_contentMeta parameters:

| Name | Type | Description |
| --- | --- | --- |
| trust | Object | An object with the fieldData array that contains details about the fields participating in the trust score calculation.  The fieldData array contains the following parameters:   * fieldRef. Fields that have different trust scores. The value "/" indicates that all fields in the record share the same trust score. * lastUpdatedDate. Date when the field value was last updated. * manualOverride. Indicates whether a field value was manually overridden. * sourcePrimaryKey. Primary key of the source record. * sourceSystem. Internal ID of the source system to which the record belongs. * trustRule. Survivorship configuration that identified the best value for the field. * trustScore. Latest trust score of the field value. |
| ruleAssociation | Object | An object that contains details of failed or in-progress rule associations.  The object includes the fail array that contains details about the failed or in-progress rule associations and the fields to which these rules were applied.  The fail array contains the following parameters:   * provider. For internal use only. * rule. Array that contains details about failed or in-progress rule associations.  The array contains the following parameters:   + id. Internal ID of the failed or in-progress rule association. + fieldRef. Array that contains the fields to which the rule associations were applied. + dynErrorDetail. Error message that appears when a custom rule specification is configured from Data Quality. |

### GET examples

You might use the following request to retrieve the business entity master record associated with the c360.default.system source system and the pkzkey31 primary key:

GET <baseApiURL>/business-entity/public/api/v1/entity/c360.person/c360.default.system/pkzkey3?\_showContentMeta=true

The following sample response shows the active master record that matches the specified source name and sourcePKey:

{  
 "firstName": "James",  
 "middleName": "Mc",  
 "lastName": "Gill",  
 "fullName": "James MC Gill",  
 "gender": {  
 "Code": "M",  
 "Name": "Male"  
 },  
 "\_meta": {  
 "id": "683d6537fe40f71fb1f7dd5f",  
 "businessEntity": "c360.person",  
 "businessId": "MDM0000000I05J",  
 "state": "ACTIVE",  
 "validation": "IN\_PROGRESS",  
 "createdBy": "tanmaya",  
 "creationDate": 1748510430175,  
 "updatedBy": "tanmaya",  
 "lastUpdateDate": 1748510430175  
 },  
 "\_contentMeta": {  
 "trust": {  
 "fieldData": [  
 {  
 "fieldRef": "middleName",  
 "lastUpdatedDate": 1748510430175,  
 "manualOverride": false,  
 "sourcePrimaryKey": "683826de176d8555c1172fd7",  
 "sourceSystem": "c360.default.system",  
 "trustRule": "SourceRanking",  
 "trustScore": 0.9  
 },  
 {  
 "fieldRef": "/",  
 "lastUpdatedDate": 1748510430175,  
 "manualOverride": false,  
 "sourcePrimaryKey": "683826de176d8555c1172fd7",  
 "sourceSystem": "c360.default.system",  
 "trustRule": "SourceRanking",  
 "trustScore": 1.0  
 }  
 ]  
 },  
 "ruleAssociation": {  
 "fail": [  
 {  
 "provider": "mdm-business-entity",  
 "rule": [  
 {  
 "id": "min5\_middlename\_1743147609745",  
 "fieldRef": [  
 "middleName"  
 ],  
   
 }  
 ]  
 }  
 ],  
 "exception": []  
 }  
 }  
}

You might use the following request to retrieve the pending business entity master record associated with the c360.default.system source system and the pendpkzkey32 primary key:

GET <baseApiURL>/business-entity/public/api/v1/entity/c360.person/c360.default.system/pendpkzkey32?\_showContentMeta=true&\_showPending=true

The following sample response shows the pending master record that matches the specified source name and sourcePKey:

{  
 "firstName": "Anita",  
 "middleName": "Jane",  
 "lastName": "Kumar",  
 "fullName": "Anita Jane Kumar",  
 "\_meta": {  
 "id": "MDM0000000PXN5",  
 "businessEntity": "c360.person",  
 "businessId": "MDM0000000PXN5",  
 "state": "PENDING\_CREATE",  
 "validation": "PASSED",  
 "consolidation": "MATCH\_INDEXED",  
 "searchIndex": "SEARCH\_INDEXED",  
 "createdBy": "Impact",  
 "creationDate": 1756473570942,  
 "updatedBy": "Impact",  
 "lastUpdateDate": 1756473570942,  
 "changeList": "68b1a8e24fb65c72f80df3c0"  
 },  
 "\_contentMeta": {  
 "trust": {  
 "fieldData": [  
 {  
 "fieldRef": "middleName",  
 "lastUpdatedDate": 1756473570942,  
 "manualOverride": false,  
 "sourcePrimaryKey": "683826de176d8555c1172fd7",  
 "sourceSystem": "c360.default.system",  
 "trustRule": "SourceRanking",  
 "trustScore": 0.9  
 },  
 {  
 "fieldRef": "/",  
 "lastUpdatedDate": 1748510430175,  
 "manualOverride": false,  
 "sourcePrimaryKey": "683826de176d8555c1172fd7",  
 "sourceSystem": "c360.default.system",  
 "trustRule": "SourceRanking",  
 "trustScore": 1.0  
 }  
 ]  
 }  
 }  
}

For more information about content metadata, see [Read content meta](Read_content_meta.md "Read content meta") .

## Update Master Record

Use the Update Master Record API to update an existing active business entity record based on the business ID that you specify.

Before you update a deleted master record, restore it by using the Restore Master Record API.

For more information about the Restore Master Record API, see [Restore Master Record](Business_entity_record_APIs.md#ww3_6_46_29_1 "Business entity record APIs").

The API supports the PUT method.

### PUT request

To update an active master record associated with a business ID, submit a PUT request with the following URI:

PUT <baseApiURL>/business-entity/public/api/v1/entity/<businessEntity>/<businessId>?sourceSystem=<sourceSystem>[&\_resolveCrosswalk=true|false]

The PUT request contains the following parameters that you can use in the URI:

| Parameter | Description |
| --- | --- |
| businessEntity | Internal ID of the business entity for which you want to update the master record. |
| businessId | Unique identifier of the business entity record.  When you specify a custom business ID for records, ensure that its length doesn't match the length that the business entity uses for the business ID format. For example, if you specified 111 as the prefix and 15 as the length in the business ID format for a business entity, ensure that you don't use 111560000800000 as a custom business ID. However, you can use a custom business ID with a prefix of 111 but with a different length.  For more information about the business ID format, see [Business ID format](https://www4.informatica.com/onlinehelp/default.aspx?product=IICS/prod&ver=b360&lang=en&contextID=BUSINESS_ID_FORMAT). |
| sourceSystem | Internal ID of the source system to which the record belongs. |
| \_resolveCrosswalk | Optional. Indicates whether to standardize picklist values based on the source system configuration.  To standardize the picklist values, set to true. If set to true, ensure that the picklist values belong to a code list that's part of a crosswalk.  Default is false.  For more information about standardizing picklist values, see Define Source Systems. |

If a business entity field in a field group has a data quality rule, specify the \_id parameter value for the field group entry that you create. For example, if the Postal Code field in the Address field group has a data quality rule, specify an ID for an address entry that you create.

For more information about data quality rules, see Configure Data Quality.

### PUT response

The response is in the JSON format. The PUT response returns HTTP 200 success code. When you submit a REST API request to update a record, Business 360 verifies your existing user role permissions and the approval workflow configuration. The response indicates whether a data steward must approve the records you updated.

### PUT examples

The following request updates a master record for the c360.organization business entity:

PUT <baseApiUrl>/business-entity/public/api/v1/entity/c360.organization/30135099501325521983619650821?sourceSystem=c360.default.system&\_resolveCrosswalk=true  
Content-Type: application/json  
IDS-SESSION-ID: XXXXXXXXXXXXXXXXXXXXXX   
  
{  
 "name": "NewElectronics",  
 "numberOfEmployees": 12,  
 "companyType": {  
 "Code": "Chartered"  
 },  
 "dateOfIncorporation": "2021-06-01",  
 "TaxDetail": [  
 {  
 "taxNumberType": {  
 "Code": "ITIN"  
 },  
 "taxNumber": "12"  
 }  
 ]  
}  
}

If no approval workflow is configured, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired":false  
}

If the approval workflow is configured, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired":true  
}

## Update Master Record Field

Use the Update Master Record API to add, replace, or remove a field in an existing business entity record based on the business ID that you specify.

The API supports the PATCH method.

### PATCH request

To update a business entity master record associated with a business ID, submit a PATCH request with the following URI:

PATCH <baseApiURL>/business-entity/public/api/v1/entity/<businessEntity>/<businessId>?sourceSystem=<sourceSystem>[&\_resolveCrosswalk=true|false]

The PATCH request contains the following parameters that you can use in the URI:

| Parameter | Description |
| --- | --- |
| businessEntity | Internal ID of the business entity for which you want to update the master record. |
| businessId | Unique identifier of the business entity record.  If you specify the business ID of an inactive merged master record, the active master record, to which the inactive master record is merged, is updated. |
| sourceSystem | Internal ID of the source system to which the record belongs. |
| \_resolveCrosswalk | Indicates whether to standardize picklist values based on the source system configuration.  To standardize the picklist values, set to true. If set to true, ensure that the picklist values belong to a code list that's part of a crosswalk.  Default is false.  For more information about standardizing picklist values, see Define Source Systems. |

The PATCH request body includes the following parameters:

| Parameter | Description |
| --- | --- |
| op | The operator that indicates the type of update. You can specify the following operators:   * add. Adds new data to the record. * replace. Replaces the existing data. * remove. Removes the data from the record. |
| path | The name of the field you want to update.  **Note:**  If the field contains multiple values, specify the name and the internal ID of the field. For example, the Postal Address field contains multiple values. |
| value | Contains the values that you want to use to update the record. |

### PATCH response

The response is in the JSON format. The PATCH response returns HTTP 200 success code. When you submit a REST API request to update a record, Business 360 verifies your existing user role permissions and the approval workflow configuration. The response indicates whether a data steward must approve the records you updated.

### PATCH examples

The following request updates the first name of a person to Marko:

PATCH <baseApiUrl>/business-entity/public/api/v1/entity/c360.person/30007167057089073281821852231?sourceSystem=c360.default.system&\_resolveCrosswalk=true  
Content-Type: application/json  
IDS-SESSION-ID: XXXXXXXXXXXXXXXXXXXXXX  
  
  
[  
 {  
 "op":"replace",  
 "path":"firstName",  
 "value":"Marko"  
 }  
]

The following request adds a postal address, replaces a postal address, and removes a postal address:

[{  
 "op": "add",  
 "path": "PostalAddress",  
 "value": {  
 "\_id": "60f01672b9bac05632a3111111",  
 "addressLine1": "Nikola Tesla Avenue",  
 "country": {  
 "Code": "US",  
 "Name": "America"  
 },  
 "state": {  
 "Code": "CA",  
 "Name": "California"  
 }  
 }  
 },  
 {  
 "op": "replace",  
 "path": "PostalAddress[1ebb60fbf93d41518f86f79f7211c3be]",  
 "value": {  
 "\_id": "1ebb60fbf93d41518f86f79f7211c3be",  
 "addressLine1": "Cambie Street",  
 "country": {  
 "Code": "US",  
 "Name": "America"  
 }  
 }  
 },  
 {  
 "op": "remove",  
 "path": "PostalAddress[60f01672b9bac05632a3d522]"  
 }  
]

The following PATCH request stops the inheritance of the Color dynamic field definition and adds the Material dynamic field definition to a category record:

[  
 {  
 "op": "replace",  
 "path": "product\_dynamic\_field\_definitions[color\_001\_dfd1]",  
 "value": {  
 "template": "color",  
 "mandatory": true,  
 "allowMany": true,  
 "inheritance": [  
 {  
 "mode": "STOP"  
 }  
 ]  
 }  
 },  
 {  
 "op": "add",  
 "path": "product\_dynamic\_field\_definitions[material\_property\_001]",  
 "value": {  
 "template": "material",  
 "mandatory": false,  
 "allowMany": false  
 }  
 }  
]

The following PATCH request adds and updates dynamic field values:

[  
 {  
 "op": "replace",  
 "path": "product\_dynamic\_field\_values.color",  
 "value": "Blue"  
 },  
 {  
 "op": "add",  
 "path": "product\_dynamic\_field\_values.manufacturing\_details[dbb63393e71b42\_entry1]",  
 "value": {   
 "brand" : "JBL" ,  
 "country\_of\_origin":"US"  
 }  
 },  
 {  
 "op": "replace",  
 "path": "product\_dynamic\_field\_values",  
 "value": {  
 "dimensions": [  
 {  
 "\_id": "dbb63393e71b424e8873d0e921622e1e",  
 "width": "950 mm",  
 "height":"67 mm"  
 }  
 ]  
 }  
 }  
]

If no approval workflow is configured, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired":false  
}

If the approval workflow is configured, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired":true  
}

## Read Source Records by Master Record

Use the Read Source Records by Master Record API to identify all the source records that contribute to a master record.

Before you begin, you must know the business ID of the master record. To get the business ID of a master record, you can submit a search request.

The API supports the GET method.

### GET request

To get the source records that contribute to a master record, submit a GET request with the following URI:

GET <baseApiURL>/business-entity/public/api/v1/entity-member/<businessEntity>/<businessId>[&\_skipLookup=true|false]

The GET request contains the following parameters that you can use in the URI:

| Parameter | Description |
| --- | --- |
| businessEntity | Internal ID of the business entity to which the master record belongs. |
| businessId | Unique identifier of the master record whose source records you want to retrieve. |
| \_skipLookup | Optional. Indicates whether to retrieve picklist values from the Business 360 data store or Reference 360 data store. The response time improves when you retrieve values from the Business 360 data store.  To retrieve picklist values from the Business 360 data store, set to true. The picklist values are based on the data from the last updated record and might not include subsequent changes to the Reference 360 data. To retrieve picklist values from the Reference 360 data store, set to false. Default is false. |

### GET response

The response is in the JSON format. The response body contains the source records that contribute to the specified master record.

| Name | Type | Description |
| --- | --- | --- |
| sourcePKey | String | Primary source key of the source record. |
| sourceSystem | String | Unique identifier of the source system for the source record. |

### GET examples

You might use the following request to retrieve the business entity source member records based on the business ID MDM0000007QMV4:

GET <baseURL>/business-entity/public/api/v1/entity-member/c360.person/  
MDM0000007QMV4

The sample response retrieves the source records that contribute to the master record that matches the specified business ID.

{  
 "content": [  
 {  
 "sourcePKey": "ex62854adaf03192610b6ad9c0",  
 "sourceSystem": "c360.default.system"  
 },  
 {  
 "sourcePKey": "ex628547194fa64531c105625f",  
 "sourceSystem": "c360.default.system"  
 },  
 {  
 "sourcePKey": "001",  
 "sourceSystem": "microsoft"  
 },  
 {  
 "sourcePKey": "ex628549c94fa64531c105631c",  
 "sourceSystem": "c360.default.system"  
 },  
 {  
 "sourcePKey": "001",  
 "sourceSystem": "salesforce"  
 }  
 ]  
}

## Delete Master Record

Use the Delete Master Record API to delete an active master record based on the business ID of the record. The API doesn't permanently delete the record from the data store, and you can search for the deleted record. You can also use the Delete Master Record API to delete an active source record based on the primary key and the source system of the source record.

The API supports the DELETE method.

### DELETE request

To delete a business entity master record, submit a DELETE request with the following URI:

DELETE <baseApiURL>/business-entity/public/api/v1/entity/<businessEntity>/<businessId>?sourceSystem=<source\_system>

To delete a business entity source record, submit a DELETE request with the following URI:

DELETE <baseApiURL>/business-entity/public/api/v1/entity/<businessEntity>/<businessId>?sourceSystem=<source\_system>&sourcePKey=<sourcePKey>

Use the following parameters in the DELETE request:

| Parameter | Description |
| --- | --- |
| businessEntity | Internal ID of the business entity for which you want to delete the master record. |
| businessId | Unique identifier of the master record. |
| sourceSystem | Internal ID of the source system that the record belongs to. |
| sourcePKey | The source primary key of the record that you want to delete. |

### DELETE response

The DELETE response returns the HTTP 200 success code. When you submit a REST API request to delete a record, MDM SaaS verifies the permissions of your user role and the approval workflow configuration. The response indicates whether the deletion request requires an approval.

If no approval workflow is configured for your user role, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired":false  
}

If an approval workflow is configured for your user role, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired":true  
}

### DELETE examples

You can use the following request to delete an active master record based on the business ID:

DELETE <baseApiURL>/business-entity/public/api/v1/entity/c360.person/MDM0000007DK79?sourceSystem=c360.default.system

If no approval workflow is configured, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired": false,  
}

If an approval workflow is configured, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired": true,  
}

You can use the following request to delete an active source record:

DELETE <baseApiURL>/business-entity/public/api/v1/entity/c360.person/MDM0000007DK79?sourceSystem=c360.default.system&sourcePKey=SFDC\_566

If no approval workflow is configured, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired": false,  
}

If an approval workflow is configured, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired": true,  
}

## Restore Master Record

Use the Restore Master Record API to restore a deleted master record. You can restore a deleted master record if it has at least one active source record.

The API supports the PUT method.

### PUT request

To restore a deleted master record, submit a PUT request with the following URI:

PUT <baseApiURL>/business-entity/public/api/v1/entity/<businessEntity>/<businessId>?sourceSystem=<source\_system>&\_operationType=restore

Use the following parameters in the PUT request:

| Parameter | Description |
| --- | --- |
| businessEntity | Internal ID of the business entity for which you want to restore a deleted master record. |
| businessId | Unique identifier of the deleted master record that you want to restore. |
| sourceSystem | Internal ID of the source system that the record belongs to. |
| \_operationType | Type of operation that you want to perform on the deleted master record. The supported value is restore. |

### Request body

You must include a blank request body {} for the PUT request to succeed.

### PUT response

The PUT response returns the HTTP 200 success code. When you submit a REST API request to restore a master record, MDM SaaS verifies the permissions of your user role and the approval workflow configuration. The response indicates whether the restore request requires an approval.

If no approval workflow is configured for your user role, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired":false  
}

If an approval workflow is configured for your user role, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired":true  
}

### PUT example

To restore a deleted master record, you can submit the following request:

PUT <baseApiURL>/business-entity/public/api/v1/entity/c360.person/MDM0000007DK79?sourceSystem=c360.default.system&\_operationType=restore

Include a blank request body in the following format:

{}

If no approval workflow is configured, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired": false,  
}

If an approval workflow is configured, you get the following response:

HTTP/1.1 200 OK  
Content-Type: application/json  
  
{  
 "approvalRequired": true,  
}

## Related Entities API

Use the Related Entities API to retrieve hierarchical and non-hierarchical relationships along with related records of a master record based on its business ID. You can filter the relationships based on the relationship type.

You can retrieve maximum of 10,000 relationships.

**Note:** 

You canât retrieve relationships established through a business entity record attribute.

To retrieve relationships of a master record, submit a POST request with the following URI:

<baseApiUrl>/business-entity/public/api/v1/related-entities/<businessId>

To retrieve relationships, you can use the following query parameter in the URI:

| Parameter | Type | Description |
| --- | --- | --- |
| businessId | String | Unique identifier of the master record. |

### Request body

If you're uncertain about the business entity to which the master record belongs, you can provide a list of business entities that can contain the business ID of the master record. If a business ID belongs to multiple business entities, the API doesn't return any response.

You can use the following parameters in the request body:

| Parameter | Type | Description |
| --- | --- | --- |
| \_page | Integer | Maximum number of pages that can contain the retrieved relationships. Specify a value between 1 and 10,000.  You can retrieve up to 10,000 relationships. If the number of relationships that can appear on each page is 5, then the page size is set to 2,000 to retrieve 10,000 relationships. |
| \_size | Integer | Maximum number of relationships that can appear on each page. Specify a value up to 50. |
| \_anchorBusinessEntity | String | Comma-separated list of internal IDs of business entities that can contain the business ID of the master record. You can specify up to 10 business entities. |
| \_relationship | String | Optional. Comma-separated list of internal IDs of relationships that you want to use to filter the response. |

### Request response

The request response returns the relationships and related records associated with the master record.

The following table lists the response parameters:

| Parameter | Type | Description |
| --- | --- | --- |
| content | Array | Array that contains relationship details.  The array contains the following parameters:   * relation. Contains the relationship details.  + \_from. Business entity and the business ID of the record from which the relationship starts. + \_to. Business entity and the business ID of the record to which the relationship is established. + \_meta. Contains the metadata of the relationship that relates the records.  * targetEntity. Contains the details of related records.  + \_meta. Contains the metadata of related records. |
| resultsInPage | Integer | Number of related records on the current page. |
| size | Integer | Requested page size that you specify in the request. |
| page | Integer | Number indicating the page that's retrieved. |

The POST response includes the following \_meta parameters for relationships:

| Parameter | Type | Description |
| --- | --- | --- |
| relationship | String | Internal ID of the relationship. |
| createdBy | String | Name of the user who created the relationship. |
| creationDate | String | Date when the relationship was created. |
| updatedBy | String | Name of the user who last updated the relationship. |
| lastUpdateDate | String | Date when the relationship was last updated. |
| businessId | String | Unique identifier of the relationship. |
| state | String | Current state of the specified master record, such as active. |

The POST response includes the following \_meta parameters for related records:

| Parameter | Type | Description |
| --- | --- | --- |
| businessEntity | String | Internal ID of the business entity to which the related record belongs. |
| createdBy | String | Name of the user who created the related record. |
| creationDate | String | Date when the related record was created. |
| updatedBy | String | Name of the user who last updated the related record. |
| lastUpdateDate | String | Date when the related record was last updated. |
| businessId | String | Unique identifier of the related record. |
| state | String | Current state of the related record, such as active. |
| consolidation | String | Match and merge status of the related record. |
| validation | String | Validation status of the related record. |

### Sample Request

You might use the following request to retrieve relationships and related records of a master record with the business ID MDM00000009N81:

POST <baseApiURL>/business-entity/public/api/v1/related-entities/MDM00000009N81

{  
 "\_page": 0,  
 "\_size":50,  
 "\_anchorBusinessEntity": [  
 "c360.person",  
 "c360.organization"  
 ],  
 "\_relationship": {  
 "type": [  
 "c360\_person\_to\_c360\_organization",  
 "c360\_organization\_to\_c360\_person",  
 "c360\_person\_to\_c360\_person",  
 "c360.rel.personemployeeorganization",  
 "c360.rel.personchildperson",  
 "colleague"  
 ]  
 }  
}

### Sample Response

The response includes the related records that match the criteria that you specify.

The POST response returns an HTTP 200 success code and includes the relationships and related records of the master record.

{  
 "content": [  
 {  
 "relation": {  
 "\_from": {  
 "businessEntity": "c360.person",  
 "businessId": "MDM00000009FIB"  
 },  
 "\_to": {  
 "businessEntity": "c360.person",  
 "businessId": "MDM00000009FI8"  
 },  
 "\_meta": {  
 "relationship": "colleague",  
 "createdBy": "John",  
 "creationDate": 1740984999034,  
 "updatedBy": "John",  
 "lastUpdateDate": 1740984999034,  
 "businessId": "MDM000000007PT",  
 "state": "ACTIVE"  
 }  
 },  
 "targetEntity": {  
 "firstName": "Henry",  
 "lastName": "Scot",  
 "fullName": "Henry Scot",  
 "\_meta": {  
 "businessEntity": "c360.person",  
 "createdBy": "John",  
 "creationDate": 1740984999034,  
 "updatedBy": "John",  
 "lastUpdateDate": 1740985001724,  
 "businessId": "MDM00000009FIB",  
 "state": "ACTIVE",  
 "consolidation": "MATCH\_INDEXED",  
 "validation": "PASSED"  
 }  
 }  
 },  
 {  
 "relation": {  
 "\_from": {  
 "businessEntity": "c360.person",  
 "businessId": "MDM00000009FI8"  
 },  
 "\_to": {  
 "businessEntity": "c360.organization",  
 "businessId": "MDM00000002SB4"  
 },  
 "\_meta": {  
 "relationship": "c360.rel.personemployeeorganization",  
 "createdBy": "John",  
 "creationDate": 1740974689771,  
 "updatedBy": "John",  
 "lastUpdateDate": 1740974689771,  
 "businessId": "MDM00000000UV4",  
 "state": "ACTIVE"  
 }  
 },  
 "targetEntity": {  
 "name": "Museum of art",  
 "numberOfEmployees": 6879,  
 "dateOfIncorporation": "2024-10-28",  
 "companyType": {  
 "Code": "Non-Government",  
 "Name": "Non-Government"  
 },  
 "countryOfIncorporation": {  
 "Code": "ZW",  
 "Name": "Zimbabwe"  
 },  
 "\_meta": {  
 "businessEntity": "c360.organization",  
 "createdBy": "John",  
 "creationDate": 1740974669336,  
 "updatedBy": "John",  
 "lastUpdateDate": 1740974672159,  
 "businessId": "MDM00000002SB4",  
 "state": "ACTIVE",  
 "consolidation": "MATCH\_INDEXED",  
 "validation": "PASSED"  
 }  
 }  
 }  
 ],  
 "resultsInPage": 2,  
 "size": 50,  
 "page": 0  
}

## Anonymize History of Master Record API

Use the Anonymize History of Master Record API to anonymize the history of a master record, ensuring that sensitive information is protected and complies with privacy regulations. You can apply anonymization to remove personally identifiable information (PII) from the history of master records while preserving the integrity of data for analysis and reporting.

The Anonymize History of Master Record API anonymizes the historical data associated with a master record based on the source record details that you provide. The API finds data from a source record in the history of the master record and anonymizes that data.

**Note:** 

Effective in the May 2025 release, the ability to anonymize the history data of a master record is available for preview.

Preview functionality is supported for evaluation purposes but is unwarranted and is not supported in production environments or any environment that you plan to push to production. Informatica intends to include the preview functionality in an upcoming release for production use, but might choose not to in accordance with changing market or technical circumstances. For more information, contact Informatica Global Customer Support.

The API supports the POST method.

### POST request

To anonymize the history of a master record, submit a POST request with the following URI:

POST <baseApiUrl>/business-entity/public/api/v1/entity-history/<businessEntity>/anonymize

The POST request contains the following parameters:

| Parameter | Description |
| --- | --- |
| businessEntity | Internal ID of the business entity to which the master record that you want to anonymize belongs. |

### Request body

You can use the following parameters in the request body:

| Parameter | Type | Description |
| --- | --- | --- |
| sourceSystem | String | Internal ID of the source system to which the source record belongs.  The source record is associated with the history of the master record. |
| sourcePKey | String | The primary key of the source record that's associated with the master record. |
| fields | String | The field values that you want to use to anonymize the history of a master record. Include a field name and the anonymized value. |
| redoAnonymizedBefore: <Time stamp> | Epoch | Optional. Indicates that you are re-anonymizing the source record. It can be new values that aren't anonymized or already anonymized field values. The history of master records anonymized before the specified timestamp are re-anonymized.  Specify the current timestamp in the epoch format. |

### POST response

The response is in the JSON format. The POST response returns HTTP 204 No Content.

### POST example

The following sample request re-anonymizes the first name and last name in the history of a master record associated with the specified source record:

POST <baseApiUrl>/business-entity/public/api/v1/entity-history/c360.person/anonymize  
{  
 "sourceSystem": "demo.system.default",  
 "sourcePKey": "001",  
 "fields": {  
 "firstName": "XXX",  
 "lastName": "XXX"  
 }  
 "redoAnonymizedBefore": 1721427778000  
}

The first name and last name values updated before the provided timestamp are anonymized.

## Direct link API

Use the direct link API to securely access master records from external applications. You can open master records in business applications directly from external applications if you have access to the records and business applications. The direct link remains valid until the records are purged.

### Request

To view the records from an external application, use the direct link API with the following URI:

https://<baseApiUrl>/ui-x360-app/app/<businessApplication>/public/<businessEntity>/<businessId>

Ensure that you have a valid session. The REST API uses HTTP authentication method to authenticate the redirection link. The redirection link automatically directs you from external application to MDM SaaS.

The direct link API request contains the following parameters that you can use in the URI:

| Name | Description |
| --- | --- |
| businessApplication | Internal ID of the business application that the business entity belongs to.  Use one of the following values:   * c360. Indicates that the record belongs to Customer 360 SaaS. * p360. Indicates that the record belongs to Product 360 SaaS. * s360. Indicates that the record belongs to Supplier 360 SaaS. * mde. Indicates that the record belongs to Multidomain MDM SaaS. |
| businessEntity | Internal ID of the business entity that the master record belongs to. |
| businessId | Unique identifier of the master record. |

### Response

When you submit a REST API request to view a master record, MDM SaaS verifies your credentials and opens the record details page of the master record.

### Example

You might use the following request to open a master record in Customer 360 SaaS based on the business ID MDM0005600GXKU:

https://<baseApiURL>/ui-x360-app/app/c360/public/c360.person/MDM0005600GXKU

MDM SaaS verifies your credentials and opens the record details page of the master record.